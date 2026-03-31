import { useEffect, useState } from "react";

import { get_card_face_data, get_card_parse_details } from "@wasm/engine";
import { ensureCardDatabase } from "../services/cardData";

/**
 * Engine-parsed card face data returned from WASM.
 * Mirrors the Rust `CardFace` struct — same shape as card-data.json entries.
 */
export interface EngineCardFace {
  name: string;
  card_type: { supertypes: string[]; core_types: string[]; subtypes: string[] };
  oracle_text?: string | null;
  keywords: unknown[];
  abilities: unknown[];
  triggers: unknown[];
  static_abilities: unknown[];
  replacements: unknown[];
}

/**
 * A node in the engine's hierarchical parse tree for a single card.
 * Mirrors the Rust `ParsedItem` struct from `coverage.rs`.
 */
export interface ParsedItem {
  category: "keyword" | "ability" | "trigger" | "static" | "replacement" | "cost";
  label: string;
  source_text?: string | null;
  supported: boolean;
  details?: [string, string][];
  children?: ParsedItem[];
}

/**
 * Looks up a card's engine-parsed face data from the WASM card database.
 * Returns null while loading or if the card is not found.
 *
 * The card database must already be loaded (via ensureCardDatabase) — this hook
 * ensures that as a prerequisite, then calls get_card_face_data() for the lookup.
 */
export function useEngineCardData(cardName: string | null): EngineCardFace | null {
  const [data, setData] = useState<EngineCardFace | null>(null);

  useEffect(() => {
    if (!cardName) {
      setData(null);
      return;
    }

    let cancelled = false;

    ensureCardDatabase().then(() => {
      if (cancelled) return;
      const result = get_card_face_data(cardName);
      setData(result ?? null);
    });

    return () => { cancelled = true; };
  }, [cardName]);

  return data;
}

/**
 * Returns the hierarchical parse tree for a card, with per-item support status.
 * Each item includes category, label, source text, a `supported` boolean,
 * structured detail key-value pairs, and recursive children.
 *
 * This is the engine's authoritative view of what was parsed and what wasn't.
 */
export function useCardParseDetails(cardName: string | null): ParsedItem[] | null {
  const [items, setItems] = useState<ParsedItem[] | null>(null);

  useEffect(() => {
    if (!cardName) {
      setItems(null);
      return;
    }

    let cancelled = false;

    ensureCardDatabase().then(() => {
      if (cancelled) return;
      const result = get_card_parse_details(cardName);
      setItems(result ?? null);
    });

    return () => { cancelled = true; };
  }, [cardName]);

  return items;
}
