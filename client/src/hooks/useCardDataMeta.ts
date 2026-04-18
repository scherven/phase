import { useEffect, useState } from "react";

export interface CardDataMeta {
  generated_at: string;
  commit: string;
  commit_short: string;
  /** MTGJSON version string (e.g. "5.3.0+20260418") sourced from their Meta.json. */
  mtgjson_version?: string;
  /** MTGJSON snapshot date (ISO yyyy-mm-dd). */
  mtgjson_date?: string;
}

let cached: CardDataMeta | null = null;
let fetchPromise: Promise<CardDataMeta | null> | null = null;

function fetchMeta(): Promise<CardDataMeta | null> {
  if (!fetchPromise) {
    fetchPromise = fetch(__CARD_DATA_META_URL__)
      .then((res) => (res.ok ? (res.json() as Promise<CardDataMeta>) : null))
      .then((data) => {
        if (data?.generated_at) cached = data;
        return cached;
      })
      .catch(() => null);
  }
  return fetchPromise;
}

export function useCardDataMeta(): CardDataMeta | null {
  const [meta, setMeta] = useState<CardDataMeta | null>(cached);

  useEffect(() => {
    if (cached) return;
    fetchMeta().then((m) => { if (m) setMeta(m); });
  }, []);

  return meta;
}

export function formatRelativeDate(iso: string): string {
  const then = new Date(iso).getTime();
  if (isNaN(then)) return iso;

  const diffMs = Date.now() - then;
  const minutes = Math.floor(diffMs / 60_000);
  const hours = Math.floor(diffMs / 3_600_000);
  const days = Math.floor(diffMs / 86_400_000);

  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes}m ago`;
  if (hours < 24) return `${hours}h ago`;
  if (days < 30) return `${days}d ago`;
  return new Date(iso).toLocaleDateString("en-US", { month: "short", day: "numeric" });
}
