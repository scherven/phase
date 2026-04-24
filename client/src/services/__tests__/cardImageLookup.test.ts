import { describe, expect, it } from "vitest";
import { cardImageLookup } from "../cardImageLookup.ts";

describe("cardImageLookup", () => {
  it("returns front-face lookup for a plain (non-transformed) card", () => {
    expect(
      cardImageLookup({
        name: "Lightning Bolt",
        transformed: false,
        back_face: null,
      }),
    ).toEqual({ name: "Lightning Bolt", faceIndex: 0 });
  });

  it("returns front-face lookup for an untransformed DFC (back_face present but not flipped)", () => {
    expect(
      cardImageLookup({
        name: "The Legend of Kuruk",
        transformed: false,
        back_face: {
          name: "Kuruk, the Mastodon",
        } as never,
      }),
    ).toEqual({ name: "The Legend of Kuruk", faceIndex: 0 });
  });

  it("resolves a transformed permanent to the stashed front-face name + faceIndex 1", () => {
    // After transform, the engine swaps obj.name to the back-face name and
    // stashes the original front-face characteristics in obj.back_face. The
    // Scryfall data map indexes only the front-face name, so the lookup must
    // use obj.back_face.name (which holds the front name) to hit the entry.
    expect(
      cardImageLookup({
        name: "Kuruk, the Mastodon",
        transformed: true,
        back_face: {
          name: "The Legend of Kuruk",
        } as never,
      }),
    ).toEqual({ name: "The Legend of Kuruk", faceIndex: 1 });
  });

  it("falls back to obj.name when transformed but back_face is missing", () => {
    expect(
      cardImageLookup({
        name: "Kuruk, the Mastodon",
        transformed: true,
        back_face: null,
      }),
    ).toEqual({ name: "Kuruk, the Mastodon", faceIndex: 1 });
  });
});
