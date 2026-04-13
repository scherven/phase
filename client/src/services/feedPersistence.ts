import { createStore, del, entries, set } from "idb-keyval";
import { create } from "zustand";

import type { Feed } from "../types/feed";

/**
 * Feed caches live in IndexedDB rather than localStorage: a typical bundled
 * feed is 20–70KB of JSON, and localStorage's ~5MB origin quota is consumed
 * quickly once feeds are combined with per-deck entries. IndexedDB has no
 * practical size limit and matches the pattern already used for game state
 * in `gamePersistence.ts`.
 *
 * The zustand store is the in-memory source of truth for render reads; IDB
 * is the durable mirror. Writes update both. Reads go through the store.
 */

let _store: ReturnType<typeof createStore> | undefined;
function getFeedStore(): ReturnType<typeof createStore> {
  if (!_store) _store = createStore("phase-feed-cache", "phase-feed-cache");
  return _store;
}

interface FeedCacheState {
  cache: Record<string, Feed>;
  hydrated: boolean;
}

const useFeedCacheStore = create<FeedCacheState>(() => ({
  cache: {},
  hydrated: false,
}));

// ── Sync access (for non-React callers invoked post-hydration) ──────────

export function getCachedFeed(feedId: string): Feed | null {
  return useFeedCacheStore.getState().cache[feedId] ?? null;
}

export async function setCachedFeed(feedId: string, feed: Feed): Promise<void> {
  useFeedCacheStore.setState((s) => ({ cache: { ...s.cache, [feedId]: feed } }));
  try {
    await set(feedId, feed, getFeedStore());
  } catch (err) {
    console.warn(`[setCachedFeed] IDB write failed for "${feedId}":`, err);
  }
}

export function removeCachedFeed(feedId: string): void {
  useFeedCacheStore.setState((s) => {
    if (!(feedId in s.cache)) return s;
    const next = { ...s.cache };
    delete next[feedId];
    return { cache: next };
  });
  void del(feedId, getFeedStore()).catch((err) => {
    console.warn(`[removeCachedFeed] IDB delete failed for "${feedId}":`, err);
  });
}

// ── Reactive hooks (for render-path consumers) ──────────────────────────

export function useCachedFeed(feedId: string): Feed | null {
  return useFeedCacheStore((s) => s.cache[feedId] ?? null);
}

export function useFeedCacheSnapshot(): Record<string, Feed> {
  return useFeedCacheStore((s) => s.cache);
}

// ── Hydration + legacy migration ────────────────────────────────────────

const LEGACY_FEED_CACHE_PREFIX = "phase-feed:";

export async function hydrateFeedCache(): Promise<void> {
  if (useFeedCacheStore.getState().hydrated) return;

  let fromIdb: Record<string, Feed> = {};
  try {
    const rows = (await entries(getFeedStore())) as Array<[IDBValidKey, Feed]>;
    for (const [id, feed] of rows) fromIdb[String(id)] = feed;
  } catch (err) {
    console.warn("[hydrateFeedCache] IDB read failed:", err);
    fromIdb = {};
  }

  const fromLegacy = await migrateLegacyFeedCache();
  useFeedCacheStore.setState({
    cache: { ...fromIdb, ...fromLegacy },
    hydrated: true,
  });
}

async function migrateLegacyFeedCache(): Promise<Record<string, Feed>> {
  const collected: Record<string, Feed> = {};
  const legacyKeys: string[] = [];

  for (let i = 0; i < localStorage.length; i++) {
    const key = localStorage.key(i);
    if (key?.startsWith(LEGACY_FEED_CACHE_PREFIX)) legacyKeys.push(key);
  }

  for (const key of legacyKeys) {
    const raw = localStorage.getItem(key);
    if (raw) {
      try {
        collected[key.slice(LEGACY_FEED_CACHE_PREFIX.length)] = JSON.parse(raw) as Feed;
      } catch {
        /* skip malformed legacy entry */
      }
    }
    localStorage.removeItem(key);
  }

  for (const [id, feed] of Object.entries(collected)) {
    try {
      await set(id, feed, getFeedStore());
    } catch (err) {
      console.warn(`[migrateLegacyFeedCache] IDB write failed for "${id}":`, err);
    }
  }

  return collected;
}

// ── Test helpers ────────────────────────────────────────────────────────

/** @internal Reset the in-memory cache. Tests only. */
export function _resetFeedCacheForTests(): void {
  useFeedCacheStore.setState({ cache: {}, hydrated: false });
}
