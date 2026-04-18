type EngineModule = typeof import("@wasm/engine");

let engineModulePromise: Promise<EngineModule> | null = null;
let wasmInitPromise: Promise<void> | null = null;
let cardDbPromise: Promise<number> | null = null;

async function loadEngineModule(): Promise<EngineModule> {
  if (!engineModulePromise) {
    engineModulePromise = import("@wasm/engine");
  }
  return engineModulePromise;
}

export async function ensureWasmInit(): Promise<void> {
  if (!wasmInitPromise) {
    wasmInitPromise = (async () => {
      const engine = await loadEngineModule();
      await engine.default();
    })();
  }
  return wasmInitPromise;
}

export async function ensureCardDatabase(): Promise<number> {
  if (!cardDbPromise) {
    cardDbPromise = (async () => {
      await ensureWasmInit();
      const engine = await loadEngineModule();
      const resp = await fetch(__CARD_DATA_URL__);
      if (!resp.ok) {
        throw new Error(`Failed to load card-data.json (${resp.status})`);
      }
      const text = await resp.text();
      return engine.load_card_database(text);
    })();
  }
  return cardDbPromise;
}

export async function getCardFaceData(cardName: string) {
  await ensureCardDatabase();
  const engine = await loadEngineModule();
  return engine.get_card_face_data(cardName);
}

export async function getCardParseDetails(cardName: string) {
  await ensureCardDatabase();
  const engine = await loadEngineModule();
  return engine.get_card_parse_details(cardName);
}

export async function getCardRulings(cardName: string): Promise<CardRuling[]> {
  await ensureCardDatabase();
  const engine = await loadEngineModule();
  return (engine.get_card_rulings(cardName) as CardRuling[]) ?? [];
}

/** An official WotC ruling: date + body text. Mirrors the Rust `Ruling` struct. */
export interface CardRuling {
  date: string;
  text: string;
}

export async function evaluateDeckCompatibilityJs(request: unknown) {
  await ensureCardDatabase();
  const engine = await loadEngineModule();
  return engine.evaluate_deck_compatibility_js(request);
}

/// CR 903.3: Whether the named card can be a commander
/// (legendary creature, legendary background, or "can be your commander").
export async function isCardCommanderEligible(name: string): Promise<boolean> {
  await ensureCardDatabase();
  const engine = await loadEngineModule();
  return engine.is_card_commander_eligible(name);
}
