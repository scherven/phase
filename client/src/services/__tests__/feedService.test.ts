import { describe, it, expect, beforeEach, vi } from "vitest";

vi.mock("idb-keyval", () => {
  const db = new Map<string, unknown>();
  return {
    createStore: vi.fn(() => ({})),
    get: vi.fn((key: string) => Promise.resolve(db.get(key) ?? undefined)),
    set: vi.fn((key: string, value: unknown) => {
      db.set(key, value);
      return Promise.resolve();
    }),
    del: vi.fn((key: string) => {
      db.delete(key);
      return Promise.resolve();
    }),
    entries: vi.fn(() => Promise.resolve([...db.entries()])),
    _db: db,
  };
});

import * as idbKeyval from "idb-keyval";
const getIdbDb = () => (idbKeyval as unknown as { _db: Map<string, unknown> })._db;

import {
  validateFeed,
  initializeFeeds,
  subscribe,
  unsubscribe,
  getDeckFeedOrigin,
  refreshFeed,
  adoptFeedDeck,
  listSubscriptions,
  getCachedFeed,
  getFeedDecksByFeed,
} from "../feedService";
import { _resetFeedCacheForTests } from "../feedPersistence";
import {
  STORAGE_KEY_PREFIX,
  ACTIVE_DECK_KEY,
} from "../../constants/storage";

const STARTER_FEED = {
  id: "starter-decks",
  name: "Starter Decks",
  version: 1,
  updated: "2026-03-20T00:00:00Z",
  decks: [
    {
      name: "Test Deck",
      colors: ["R"],
      main: [{ count: 4, name: "Lightning Bolt" }],
      sideboard: [],
    },
  ],
};

const COMMANDER_FEED = {
  id: "commander-precons",
  name: "Commander Precons",
  version: 1,
  updated: "2026-03-20T00:00:00Z",
  decks: [
    {
      name: "[Pre-built] Test Commander",
      colors: ["W"],
      main: [{ count: 1, name: "Sol Ring" }],
      sideboard: [],
      commander: ["Kemba, Kha Regent"],
    },
  ],
};

function makeMtgGoldfishFeed(id: string, format: string) {
  return {
    id,
    name: `${format} Meta`,
    version: 1,
    updated: "2026-03-20T00:00:00Z",
    decks: [
      {
        name: `[${format}] Top Deck`,
        colors: ["R"],
        main: [{ count: 4, name: "Lightning Bolt" }],
        sideboard: [],
      },
    ],
  };
}

const ALL_BUNDLED_FEEDS: Record<string, unknown> = {
  "starter-decks": STARTER_FEED,
  "commander-precons": COMMANDER_FEED,
  "mtggoldfish-standard": makeMtgGoldfishFeed("mtggoldfish-standard", "Standard"),
  "mtggoldfish-modern": makeMtgGoldfishFeed("mtggoldfish-modern", "Modern"),
  "mtggoldfish-pioneer": makeMtgGoldfishFeed("mtggoldfish-pioneer", "Pioneer"),
  "mtggoldfish-commander": makeMtgGoldfishFeed("mtggoldfish-commander", "Commander"),
};

const VALID_FEED = {
  id: "test-feed",
  name: "Test Feed",
  version: 1,
  updated: "2026-03-20T00:00:00Z",
  decks: [
    {
      name: "Test Deck",
      colors: ["R"],
      main: [{ count: 4, name: "Lightning Bolt" }],
      sideboard: [],
    },
    {
      name: "Another Deck",
      colors: ["U"],
      main: [{ count: 4, name: "Counterspell" }],
      sideboard: [],
    },
  ],
};

function mockFetch(data: unknown, ok = true) {
  global.fetch = vi.fn().mockResolvedValue({
    ok,
    status: ok ? 200 : 404,
    statusText: ok ? "OK" : "Not Found",
    json: () => Promise.resolve(data),
  });
}

beforeEach(() => {
  localStorage.clear();
  getIdbDb().clear();
  _resetFeedCacheForTests();
  vi.restoreAllMocks();
});

describe("validateFeed", () => {
  it("accepts a valid feed", () => {
    expect(validateFeed(VALID_FEED)).not.toBeNull();
  });

  it("rejects null", () => {
    expect(validateFeed(null)).toBeNull();
  });

  it("rejects missing id", () => {
    expect(validateFeed({ ...VALID_FEED, id: "" })).toBeNull();
  });

  it("rejects missing name", () => {
    expect(validateFeed({ ...VALID_FEED, name: "" })).toBeNull();
  });

  it("rejects non-number version", () => {
    expect(validateFeed({ ...VALID_FEED, version: "1" })).toBeNull();
  });

  it("rejects missing updated", () => {
    const { updated: _, ...noUpdated } = VALID_FEED;
    expect(validateFeed(noUpdated)).toBeNull();
  });

  it("rejects non-array decks", () => {
    expect(validateFeed({ ...VALID_FEED, decks: "not array" })).toBeNull();
  });

  it("rejects deck with missing name", () => {
    const bad = {
      ...VALID_FEED,
      decks: [{ colors: ["R"], main: [], sideboard: [] }],
    };
    expect(validateFeed(bad)).toBeNull();
  });

  it("rejects deck with invalid main entry", () => {
    const bad = {
      ...VALID_FEED,
      decks: [{
        name: "Bad",
        colors: [],
        main: [{ count: "four", name: "Bolt" }],
        sideboard: [],
      }],
    };
    expect(validateFeed(bad)).toBeNull();
  });
});

function mockFetchByUrl(feedMap: Record<string, unknown>) {
  global.fetch = vi.fn().mockImplementation((url: string) => {
    const data = Object.entries(feedMap).find(([pattern]) => url.includes(pattern))?.[1];
    return Promise.resolve({
      ok: !!data,
      status: data ? 200 : 404,
      statusText: data ? "OK" : "Not Found",
      json: () => Promise.resolve(data ?? {}),
    });
  });
}

describe("initializeFeeds", () => {
  it("subscribes to bundled feeds and seeds decks on first run", async () => {
    mockFetchByUrl(ALL_BUNDLED_FEEDS);

    await initializeFeeds();

    // Starter deck should be in localStorage
    const raw = localStorage.getItem(STORAGE_KEY_PREFIX + "Test Deck");
    expect(raw).not.toBeNull();
    const deck = JSON.parse(raw!);
    expect(deck.main[0].name).toBe("Lightning Bolt");

    // Commander deck should be in localStorage
    const cmdRaw = localStorage.getItem(STORAGE_KEY_PREFIX + "[Pre-built] Test Commander");
    expect(cmdRaw).not.toBeNull();

    // Origins tracked with registry IDs (not feed.id)
    expect(getDeckFeedOrigin("Test Deck")).toBe("starter-decks");
    expect(getDeckFeedOrigin("[Pre-built] Test Commander")).toBe("commander-precons");

    // All bundled subscriptions created
    const subs = listSubscriptions();
    expect(subs).toHaveLength(6);

  });

  it("picks up new bundled feeds on subsequent calls", async () => {
    // First call subscribes to all bundled feeds
    mockFetchByUrl(ALL_BUNDLED_FEEDS);
    await initializeFeeds();
    expect(listSubscriptions()).toHaveLength(6);

    // Second call re-fetches for updates but should not create new subscriptions
    await initializeFeeds();
    expect(listSubscriptions()).toHaveLength(6);
  });

  it("does not overwrite existing user decks", async () => {
    // User already has a deck named "Test Deck"
    localStorage.setItem(
      STORAGE_KEY_PREFIX + "Test Deck",
      JSON.stringify({ main: [{ count: 1, name: "User Card" }], sideboard: [] }),
    );

    mockFetchByUrl(ALL_BUNDLED_FEEDS);
    await initializeFeeds();

    const raw = localStorage.getItem(STORAGE_KEY_PREFIX + "Test Deck")!;
    const deck = JSON.parse(raw);
    expect(deck.main[0].name).toBe("User Card");
  });
});

describe("subscribe", () => {
  it("fetches, caches, and seeds decks for a remote URL", async () => {
    mockFetch(VALID_FEED);

    const feed = await subscribe("https://example.com/feed.json");

    expect(feed.id).toBe("test-feed");
    expect(getCachedFeed("test-feed")).not.toBeNull();
    expect(getDeckFeedOrigin("Test Deck")).toBe("test-feed");

    const subs = listSubscriptions();
    expect(subs).toHaveLength(1);
    expect(subs[0].sourceId).toBe("test-feed");
    expect(subs[0].type).toBe("remote");
  });

  it("throws on malformed feed JSON", async () => {
    mockFetch({ bad: "data" });

    await expect(subscribe("https://example.com/bad.json")).rejects.toThrow(
      "Invalid feed format",
    );
  });

  it("throws on HTTP error", async () => {
    mockFetch({}, false);

    await expect(subscribe("https://example.com/404.json")).rejects.toThrow(
      "Failed to fetch feed",
    );
  });
});

describe("unsubscribe", () => {
  it("removes cached feed, seeded decks, and origins", async () => {
    mockFetch(VALID_FEED);
    await subscribe("https://example.com/feed.json");

    // Verify decks exist
    expect(localStorage.getItem(STORAGE_KEY_PREFIX + "Test Deck")).not.toBeNull();

    unsubscribe("test-feed");

    // Decks removed
    expect(localStorage.getItem(STORAGE_KEY_PREFIX + "Test Deck")).toBeNull();
    expect(localStorage.getItem(STORAGE_KEY_PREFIX + "Another Deck")).toBeNull();

    // Cache removed
    expect(getCachedFeed("test-feed")).toBeNull();

    // Subscription removed
    expect(listSubscriptions()).toHaveLength(0);

    // Origins removed
    expect(getDeckFeedOrigin("Test Deck")).toBeNull();
  });

  it("clears active deck if it belonged to the unsubscribed feed", async () => {
    mockFetch(VALID_FEED);
    await subscribe("https://example.com/feed.json");
    localStorage.setItem(ACTIVE_DECK_KEY, "Test Deck");

    unsubscribe("test-feed");

    expect(localStorage.getItem(ACTIVE_DECK_KEY)).toBeNull();
  });
});

describe("refreshFeed", () => {
  it("updates decks with new feed data", async () => {
    mockFetch(VALID_FEED);
    await subscribe("https://example.com/feed.json");

    const updatedFeed = {
      ...VALID_FEED,
      version: 2,
      decks: [
        {
          name: "Test Deck",
          colors: ["R"],
          main: [{ count: 4, name: "Shock" }],
          sideboard: [],
        },
      ],
    };
    mockFetch(updatedFeed);

    await refreshFeed("test-feed");

    // "Test Deck" updated
    const raw = localStorage.getItem(STORAGE_KEY_PREFIX + "Test Deck")!;
    expect(JSON.parse(raw).main[0].name).toBe("Shock");

    // "Another Deck" removed (no longer in feed)
    expect(localStorage.getItem(STORAGE_KEY_PREFIX + "Another Deck")).toBeNull();
    expect(getDeckFeedOrigin("Another Deck")).toBeNull();
  });

  it("throws if not subscribed", async () => {
    await expect(refreshFeed("nonexistent")).rejects.toThrow("Not subscribed");
  });

  it("records error on fetch failure", async () => {
    mockFetch(VALID_FEED);
    await subscribe("https://example.com/feed.json");

    mockFetch({}, false);
    await expect(refreshFeed("test-feed")).rejects.toThrow();

    const subs = listSubscriptions();
    expect(subs[0].error).toBeTruthy();
  });
});

describe("adoptFeedDeck", () => {
  it("removes feed origin tracking", async () => {
    mockFetch(VALID_FEED);
    await subscribe("https://example.com/feed.json");

    expect(getDeckFeedOrigin("Test Deck")).toBe("test-feed");

    adoptFeedDeck("Test Deck");

    expect(getDeckFeedOrigin("Test Deck")).toBeNull();
    // Deck data still exists
    expect(localStorage.getItem(STORAGE_KEY_PREFIX + "Test Deck")).not.toBeNull();
  });

  it("copies deck to new name", async () => {
    mockFetch(VALID_FEED);
    await subscribe("https://example.com/feed.json");

    const result = adoptFeedDeck("Test Deck", "My Copy");

    expect(result).toBe("My Copy");
    expect(localStorage.getItem(STORAGE_KEY_PREFIX + "My Copy")).not.toBeNull();
    expect(getDeckFeedOrigin("My Copy")).toBeNull();
  });
});

describe("getFeedDecksByFeed", () => {
  it("groups deck names by feed ID", async () => {
    mockFetch(VALID_FEED);
    await subscribe("https://example.com/feed.json");

    const result = getFeedDecksByFeed();
    expect(result.get("test-feed")).toEqual(
      expect.arrayContaining(["Test Deck", "Another Deck"]),
    );
  });
});
