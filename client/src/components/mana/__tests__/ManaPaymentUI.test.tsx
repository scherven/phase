import { act } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

import { ManaPaymentUI } from "../ManaPaymentUI";
import { useGameStore } from "../../../stores/gameStore";
import type { GameState } from "../../../adapter/types";

function createGameState(overrides: Partial<GameState> = {}): GameState {
  return {
    turn_number: 1,
    active_player: 0,
    phase: "PreCombatMain",
    players: [
      { id: 0, life: 20, mana_pool: { mana: [] }, library: [], hand: [], graveyard: [], has_drawn_this_turn: false, lands_played_this_turn: 0 },
      { id: 1, life: 20, mana_pool: { mana: [] }, library: [], hand: [], graveyard: [], has_drawn_this_turn: false, lands_played_this_turn: 0 },
    ],
    priority_player: 0,
    objects: {},
    next_object_id: 1,
    battlefield: [],
    stack: [],
    exile: [],
    rng_seed: 1,
    combat: null,
    waiting_for: {
      type: "ManaPayment",
      data: { player: 0 },
    },
    lands_played_this_turn: 0,
    max_lands_per_turn: 1,
    priority_pass_count: 0,
    pending_replacement: null,
    layers_dirty: false,
    next_timestamp: 1,
    seat_order: [0, 1],
    format_config: {
      format: "Standard",
      starting_life: 20,
      min_players: 2,
      max_players: 2,
      deck_size: 60,
      singleton: false,
      command_zone: false,
      commander_damage_threshold: null,
      range_of_influence: null,
      team_based: false,
    },
    eliminated_players: [],
    ...overrides,
  };
}

describe("ManaPaymentUI", () => {
  beforeEach(() => {
    useGameStore.getState().reset();
  });

  afterEach(() => {
    cleanup();
  });

  it("renders cancel during mana payment when no top-stack spell cost can be inferred", () => {
    const dispatch = vi.fn().mockResolvedValue([]);
    const gameState = createGameState();

    act(() => {
      useGameStore.setState({
        gameState,
        waitingFor: gameState.waiting_for,
        dispatch,
        legalActions: [{ type: "CancelCast" }, { type: "PassPriority" }],
      });
    });

    render(<ManaPaymentUI />);

    expect(screen.getByText("Payment is still pending. Tap permanents or cancel this action.")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(dispatch).toHaveBeenCalledWith({ type: "CancelCast" });
  });
});
