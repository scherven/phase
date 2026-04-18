import { useEffect, useState } from "react";

import {
  type CardRuling,
  getCardFaceData,
  getCardParseDetails,
  getCardRulings,
} from "../services/engineRuntime";

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
 * The card database must already be loaded before lookup — the engine runtime
 * wrapper ensures that as a prerequisite, then performs the query.
 */
export function useEngineCardData(cardName: string | null): EngineCardFace | null {
  const [data, setData] = useState<EngineCardFace | null>(null);

  useEffect(() => {
    if (!cardName) {
      setData(null);
      return;
    }

    let cancelled = false;

    getCardFaceData(cardName).then((result) => {
      if (cancelled) return;
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

    getCardParseDetails(cardName).then((result) => {
      if (cancelled) return;
      setItems(result ?? null);
    });

    return () => { cancelled = true; };
  }, [cardName]);

  return items;
}

/**
 * Returns official WotC rulings for a card. Empty array while loading, or when
 * the card has no rulings, or for back faces of multi-face cards (rulings are
 * attached to the front face only).
 */
export function useCardRulings(cardName: string | null): CardRuling[] {
  const [rulings, setRulings] = useState<CardRuling[]>([]);

  useEffect(() => {
    if (!cardName) {
      setRulings([]);
      return;
    }

    let cancelled = false;

    getCardRulings(cardName).then((result) => {
      if (cancelled) return;
      setRulings(result);
    });

    return () => { cancelled = true; };
  }, [cardName]);

  return rulings;
}
