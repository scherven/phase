import { useEffect, useState } from "react";

/**
 * A projected MTGJSON SetList entry — the engine-side `oracle-gen set-list`
 * subcommand filters MTGJSON's ~10 MB SetList.json down to these fields only.
 * See `crates/engine/src/bin/oracle_gen.rs::SetListEntry` for the authority.
 */
export interface SetMeta {
  code: string;
  name: string;
  releaseDate?: string;
  type?: string;
  isOnlineOnly: boolean;
  baseSetSize?: number;
  /** Parent set code for draft innovation / supplemental children (e.g. MAT → MOM). */
  parentCode?: string;
}

export type SetList = Record<string, SetMeta>;

let cached: SetList | null = null;
let fetchPromise: Promise<SetList | null> | null = null;

function fetchSetList(): Promise<SetList | null> {
  if (!fetchPromise) {
    fetchPromise = fetch(__SET_LIST_URL__)
      .then((res) => (res.ok ? (res.json() as Promise<SetList>) : null))
      .then((data) => {
        if (data && typeof data === "object") cached = data;
        return cached;
      })
      .catch(() => null);
  }
  return fetchPromise;
}

/**
 * Returns the full set-list keyed by set code. `null` while loading or on
 * fetch failure. The payload is ~120 KB and is cached across hook instances.
 */
export function useSetList(): SetList | null {
  const [list, setList] = useState<SetList | null>(cached);

  useEffect(() => {
    if (cached) return;
    fetchSetList().then((l) => { if (l) setList(l); });
  }, []);

  return list;
}
