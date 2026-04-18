import { useEffect, useState } from "react";

export interface DeckCardEntry {
  name: string;
  count: number;
}

export interface DeckEntry {
  code: string;
  name: string;
  type: string;
  releaseDate?: string;
  mainBoard: DeckCardEntry[];
  sideBoard?: DeckCardEntry[];
  commander?: DeckCardEntry[];
}

export type DeckMap = Record<string, DeckEntry>;

let cached: DeckMap | null = null;
let fetchPromise: Promise<DeckMap | null> | null = null;

function fetchDecks(): Promise<DeckMap | null> {
  if (!fetchPromise) {
    fetchPromise = fetch(__DECKS_URL__)
      .then((res) => (res.ok ? (res.json() as Promise<DeckMap>) : null))
      .then((data) => {
        if (data && typeof data === "object") cached = data;
        return cached;
      })
      .catch(() => null);
  }
  return fetchPromise;
}

/**
 * Returns the preconstructed deck catalog keyed by deck id (MTGJSON filename
 * stem, e.g. `RedDeckB_10E`). Filtered at build time to decks whose every
 * card is playable by the engine. `null` while loading or on fetch failure.
 */
export function useDecks(): DeckMap | null {
  const [decks, setDecks] = useState<DeckMap | null>(cached);

  useEffect(() => {
    if (cached) return;
    fetchDecks().then((d) => { if (d) setDecks(d); });
  }, []);

  return decks;
}
