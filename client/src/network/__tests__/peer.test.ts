import { describe, it, expect, vi } from "vitest";

import { createPeerSession } from "../peer";
import { validateMessage } from "../protocol";
import { FakeDataConnection } from "./fakeDataConnection";

function createTestSession(opts?: { onSessionEnd?: () => void }) {
  const conn = new FakeDataConnection();
  // Cast to satisfy DataConnection type — we only use the subset FakeDataConnection implements.
  const session = createPeerSession(conn as never, opts);
  return { conn, session };
}

describe("P2P Protocol - validateMessage", () => {
  it("accepts valid P2P message types", () => {
    const msg = {
      type: "action",
      senderPlayerId: 1,
      action: { type: "PassPriority" },
    };
    expect(validateMessage(msg)).toEqual(msg);

    const concede = { type: "concede" };
    expect(validateMessage(concede)).toEqual(concede);

    const ping = { type: "ping", timestamp: 12345 };
    expect(validateMessage(ping)).toEqual(ping);
  });

  it("accepts new 3-4p multiplayer message types", () => {
    const types = [
      { type: "reconnect", playerToken: "abc" },
      { type: "reconnect_rejected", reason: "kicked" },
      { type: "kick", reason: "host kicked" },
      { type: "player_kicked", playerId: 2, reason: "kicked" },
      { type: "player_conceded", playerId: 2, reason: "Conceded" },
      { type: "player_disconnected", playerId: 1 },
      { type: "player_reconnected", playerId: 1 },
      { type: "game_paused", reason: "Player disconnected" },
      { type: "game_resumed" },
      { type: "lobby_progress", joined: 2, total: 3 },
    ];
    for (const msg of types) {
      expect(validateMessage(msg)).toEqual(msg);
    }
  });

  it("rejects unknown message types", () => {
    expect(() => validateMessage({ type: "unknown_garbage" })).toThrow(
      "Invalid message type",
    );
  });

  it("rejects missing type field", () => {
    expect(() => validateMessage({})).toThrow("Invalid message: missing type field");
    expect(() => validateMessage(null)).toThrow("Invalid message: missing type field");
    expect(() => validateMessage("not an object")).toThrow(
      "Invalid message: missing type field",
    );
  });
});

describe("PeerSession", () => {
  it("send returns false when connection is not open", () => {
    const { conn, session } = createTestSession();
    conn.open = false;
    const result = session.send({ type: "concede" });
    expect(result).toBe(false);
    session.close();
  });

  it("onMessage handler receives parsed messages", () => {
    const { conn, session } = createTestSession();
    const handler = vi.fn();
    session.onMessage(handler);

    const actionMessage = {
      type: "action" as const,
      senderPlayerId: 0,
      action: { type: "PassPriority" as const },
    };
    conn.simulateData(actionMessage);

    expect(handler).toHaveBeenCalledTimes(1);
    expect(handler).toHaveBeenCalledWith(actionMessage);
    session.close();
  });

  it("buffers messages when no listeners are attached, then flushes on subscribe", () => {
    const { conn, session } = createTestSession();

    const actionMessage = {
      type: "action" as const,
      senderPlayerId: 0,
      action: { type: "PassPriority" as const },
    };
    conn.simulateData(actionMessage);

    const handler = vi.fn();
    session.onMessage(handler);

    expect(handler).toHaveBeenCalledTimes(1);
    expect(handler).toHaveBeenCalledWith(actionMessage);
    session.close();
  });

  it("invokes disconnect handlers immediately if subscribed after disconnect", () => {
    const onSessionEnd = vi.fn();
    const { session } = createTestSession({ onSessionEnd });

    session.close("Peer closed");

    const handler = vi.fn();
    session.onDisconnect(handler);

    expect(handler).toHaveBeenCalledTimes(1);
    expect(handler).toHaveBeenCalledWith("Peer closed");
    expect(onSessionEnd).toHaveBeenCalledTimes(1);
  });

  it("onSessionEnd fires exactly once per session, even on cascading errors", () => {
    const onSessionEnd = vi.fn();
    const { conn, session } = createTestSession({ onSessionEnd });

    conn.simulateClose();
    conn.simulateClose(); // duplicate
    session.close("manual"); // additional close attempt

    expect(onSessionEnd).toHaveBeenCalledTimes(1);
  });
});
