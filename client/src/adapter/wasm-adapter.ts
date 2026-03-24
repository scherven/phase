import {
  ping,
  create_initial_state,
  initialize_game,
  submit_action,
  get_game_state,
  get_ai_action,
  get_legal_actions_js,
  restore_game_state,
} from "@wasm/engine";
import { ensureWasmInit, ensureCardDatabase } from "../services/cardData";
import type {
  EngineAdapter,
  FormatConfig,
  GameAction,
  GameState,
  MatchConfig,
  SubmitResult,
} from "./types";
import { AdapterError, AdapterErrorCode } from "./types";

/**
 * WASM-backed implementation of EngineAdapter.
 * Communicates directly with the Rust engine compiled to WebAssembly.
 * Serializes all WASM access through an async queue (WASM is single-threaded).
 */
export class WasmAdapter implements EngineAdapter {
  private initialized = false;
  private queue: Promise<void> = Promise.resolve();
  cardDbLoaded = false;

  async initialize(): Promise<void> {
    if (this.initialized) return;
    await ensureWasmInit();
    this.initialized = true;
  }

  /** Load the card database if not already loaded. Called before game init when deck data is provided. */
  private async ensureCardDb(): Promise<void> {
    if (this.cardDbLoaded) return;
    try {
      const count = await ensureCardDatabase();
      this.cardDbLoaded = true;
      console.log(`Card database loaded: ${count} cards`);
    } catch (err) {
      console.warn("Failed to load card database:", err);
    }
  }

  async submitAction(action: GameAction): Promise<SubmitResult> {
    this.assertInitialized();
    return this.enqueue(() => this.processAction(action));
  }

  async getState(): Promise<GameState> {
    this.assertInitialized();
    return this.enqueue(() => this.fetchState());
  }

  async getLegalActions(): Promise<GameAction[]> {
    this.assertInitialized();
    return this.enqueue(() => {
      const actions = get_legal_actions_js();
      if (actions === null) return [];
      return actions as GameAction[];
    });
  }

  getAiAction(difficulty: string, playerId = 1): Promise<GameAction | null> {
    this.assertInitialized();
    return this.enqueue(() => {
      const result = get_ai_action(difficulty, playerId);
      if (result == null) return null;
      return result as GameAction;
    });
  }

  /**
   * Get AI actions for multiple AI seats with per-seat difficulty.
   * Returns the action for the AI player whose turn it currently is, or null.
   */
  getAiActionForSeats(
    aiSeats: { playerId: number; difficulty: string }[],
    activePlayer: number,
  ): Promise<GameAction | null> {
    const seat = aiSeats.find((s) => s.playerId === activePlayer);
    if (!seat) return Promise.resolve(null);
    return this.getAiAction(seat.difficulty, seat.playerId);
  }

  restoreState(state: GameState): void {
    this.assertInitialized();
    // Enqueue to prevent RefCell borrow conflicts with in-flight operations.
    // Callers follow with enqueued getState()/getLegalActions() so ordering is preserved.
    this.enqueue(() => restore_game_state(JSON.stringify(state)));
  }

  dispose(): void {
    this.initialized = false;
    this.queue = Promise.resolve();
  }

  /** Verify WASM module is responding. */
  ping(): string {
    this.assertInitialized();
    return ping();
  }

  /** Initialize a new game and return the initial events and log entries. */
  async initializeGame(
    deckData?: unknown,
    formatConfig?: FormatConfig,
    playerCount?: number,
    matchConfig?: MatchConfig,
  ): Promise<SubmitResult> {
    this.assertInitialized();
    // Load card database on demand when deck data needs name resolution
    if (deckData) {
      await this.ensureCardDb();
    }
    const seed = Math.floor(Math.random() * Number.MAX_SAFE_INTEGER);
    return this.enqueue(() => {
      const result = initialize_game(
        deckData ?? null,
        seed,
        formatConfig ?? null,
        matchConfig ?? null,
        playerCount ?? undefined,
      );
      // Engine returns { error: true, reasons: [...] } when deck validation fails
      if (result && typeof result === "object" && "error" in result && result.error) {
        const reasons = (result as { reasons?: string[] }).reasons ?? [];
        throw new Error(`Deck validation failed: ${reasons.join("; ")}`);
      }
      return { events: result.events ?? [], log_entries: result.log_entries ?? [] };
    });
  }

  private assertInitialized(): void {
    if (!this.initialized) {
      throw new AdapterError(
        AdapterErrorCode.NOT_INITIALIZED,
        "Adapter not initialized. Call initialize() first.",
        true,
      );
    }
  }

  /**
   * Enqueue a WASM operation to ensure serialized access.
   * Each operation waits for the previous one to complete.
   */
  private enqueue<T>(operation: () => T): Promise<T> {
    const result = this.queue.then(() => {
      try {
        return operation();
      } catch (error) {
        throw this.normalizeError(error);
      }
    });
    // Update queue to track completion (ignore rejections for queue chaining)
    this.queue = result.then(
      () => undefined,
      () => undefined,
    );
    return result;
  }

  private processAction(action: GameAction): SubmitResult {
    const result = submit_action(action);
    if (typeof result === "string") {
      throw new AdapterError(
        AdapterErrorCode.INVALID_ACTION,
        result,
        true,
      );
    }
    return { events: result.events ?? [], log_entries: result.log_entries ?? [] };
  }

  private fetchState(): GameState {
    const state = get_game_state();
    if (state === null) {
      return create_initial_state() as GameState;
    }
    return state as GameState;
  }

  private normalizeError(error: unknown): AdapterError {
    if (error instanceof AdapterError) return error;

    const message =
      error instanceof Error ? error.message : String(error);
    return new AdapterError(
      AdapterErrorCode.WASM_ERROR,
      message,
      false,
    );
  }
}
