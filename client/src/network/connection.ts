import Peer from "peerjs";
import type { DataConnection } from "peerjs";

/** Unambiguous characters -- no 0/O, 1/I/L confusion */
const CODE_ALPHABET = "ABCDEFGHJKMNPQRSTUVWXYZ23456789";
const CODE_LENGTH = 5;
const PEER_ID_PREFIX = "phase-";

// Override PeerJS defaults -- their bundled TURN servers are broken
const PEER_CONFIG: RTCConfiguration = {
  iceServers: [
    { urls: "stun:stun.relay.metered.ca:80" },
    {
      urls: "turn:global.relay.metered.ca:80",
      username: "a267722d3eb02873687da73c",
      credential: "ob5D/eUnCmkkf1Vp",
    },
    {
      urls: "turn:global.relay.metered.ca:80?transport=tcp",
      username: "a267722d3eb02873687da73c",
      credential: "ob5D/eUnCmkkf1Vp",
    },
    {
      urls: "turn:global.relay.metered.ca:443",
      username: "a267722d3eb02873687da73c",
      credential: "ob5D/eUnCmkkf1Vp",
    },
    {
      urls: "turns:global.relay.metered.ca:443?transport=tcp",
      username: "a267722d3eb02873687da73c",
      credential: "ob5D/eUnCmkkf1Vp",
    },
  ],
};

export interface HostResult {
  roomCode: string;
  peerId: string;
  /**
   * The signaling-server-registered `Peer`. Exposed so the host adapter can
   * subscribe to `peer.on("connection", ...)` directly when it needs the
   * guest's PlayerId in scope at wrap time. Most callers should prefer
   * `onGuestConnected` instead — the `Peer` reference is for advanced cases.
   */
  peer: Peer;
  /**
   * Subscribe to incoming guest connections. Multi-fire: handler is called for
   * every new guest after their `DataConnection.open` event. Returns an
   * unsubscribe function.
   *
   * Each `DataConnection` is delivered to the host adapter, which is
   * responsible for wrapping it in a `PeerSession` (with its own
   * `onSessionEnd` callback) and tracking the per-guest lifecycle.
   */
  onGuestConnected: (handler: (conn: DataConnection) => void) => () => void;
  /**
   * Tear down the shared `Peer`. Sole authoritative cleanup site for the
   * underlying signaling-server connection. Per-session disconnects must NOT
   * call this — that would cascade-kill all sibling guests.
   */
  destroy: () => void;
}

export interface JoinResult {
  conn: DataConnection;
  peer: Peer;
  /** Close only the current `DataConnection` (e.g., user-initiated leave of one room while rejoining another). */
  closeConn: () => void;
  /** Tear down the entire `Peer`. Sole authoritative cleanup. Auto-reconnect must NOT call this. */
  destroyPeer: () => void;
}

export function generateRoomCode(): string {
  const chars: string[] = [];
  for (let i = 0; i < CODE_LENGTH; i++) {
    chars.push(CODE_ALPHABET[Math.floor(Math.random() * CODE_ALPHABET.length)]);
  }
  return chars.join("");
}

/**
 * Validate and normalize a room code from user input.
 * Returns the uppercase code or null if invalid.
 */
export function parseRoomCode(input: string): string | null {
  const code = input.trim().toUpperCase();
  if (code.length !== CODE_LENGTH) return null;
  for (const ch of code) {
    if (!CODE_ALPHABET.includes(ch)) return null;
  }
  return code;
}

/**
 * Host creates a room and returns a subscription handle. The host adapter
 * subscribes via `onGuestConnected` to wrap each incoming guest in a
 * `PeerSession` and track per-guest lifecycle.
 *
 * A 120s "no one joined" lobby timeout is NOT enforced here — the host
 * adapter owns lobby lifecycle (e.g., for 3-4 player games it must wait for
 * multiple guests, and the appropriate timeout depends on `playerCount`).
 *
 * Returns a Promise so the caller can await the host being registered on the
 * signaling server before exposing the room code to guests.
 */
export async function hostRoom(): Promise<HostResult> {
  const roomCode = generateRoomCode();
  const peerId = PEER_ID_PREFIX + roomCode;
  const peer = new Peer(peerId, { config: PEER_CONFIG });

  let destroyed = false;
  const guestHandlers = new Set<(conn: DataConnection) => void>();

  // Wait for the host to be registered on the signaling server.
  await new Promise<void>((resolve, reject) => {
    const onOpen = () => {
      console.log("[P2P Host] registered on signaling server, code:", roomCode);
      peer.off("error", onError);
      resolve();
    };
    const onError = (err: Error) => {
      peer.off("open", onOpen);
      reject(new Error(`Failed to create room: ${err.message}`));
    };
    peer.once("open", onOpen);
    peer.once("error", onError);
  });

  // Multi-fire connection handler: every guest gets wrapped on `open`.
  peer.on("connection", (conn) => {
    if (destroyed) {
      try { conn.close(); } catch { /* best-effort */ }
      return;
    }
    conn.on("open", () => {
      if (destroyed) {
        try { conn.close(); } catch { /* best-effort */ }
        return;
      }
      for (const handler of guestHandlers) {
        handler(conn);
      }
    });
    // Per-conn open errors are non-fatal: the parent Peer survives so other
    // guests remain connected. The PeerSession's own error handler will fire
    // for already-open connections.
    conn.on("error", (err) => {
      console.warn("[P2P Host] guest connection error (non-fatal):", err);
    });
  });

  // Top-level Peer errors: PeerJS surfaces transient issues here too. Only
  // FATAL errors should trigger destroy — transient ones are recoverable.
  peer.on("error", (err: Error & { type?: string }) => {
    const fatal = err.type === "browser-incompatible"
      || err.type === "invalid-id"
      || err.type === "invalid-key"
      || err.type === "unavailable-id"
      || err.type === "ssl-unavailable"
      || err.type === "server-error"
      || err.type === "socket-error"
      || err.type === "socket-closed";
    if (fatal) {
      console.error("[P2P Host] fatal Peer error, destroying:", err);
      destroyed = true;
      try { peer.destroy(); } catch { /* best-effort */ }
    } else {
      console.warn("[P2P Host] non-fatal Peer error:", err);
    }
  });

  return {
    roomCode,
    peerId,
    peer,
    onGuestConnected(handler) {
      guestHandlers.add(handler);
      return () => {
        guestHandlers.delete(handler);
      };
    },
    destroy() {
      destroyed = true;
      guestHandlers.clear();
      try { peer.destroy(); } catch { /* best-effort */ }
    },
  };
}

/**
 * Guest joins a room by code. Returns the `Peer` separately from the
 * `DataConnection` so the guest adapter can keep the `Peer` alive across
 * `DataConnection` drops and attempt auto-reconnect via
 * `peer.connect(hostPeerId)`.
 */
export function joinRoom(code: string): Promise<JoinResult> {
  return new Promise((resolve, reject) => {
    const peer = new Peer({ config: PEER_CONFIG });
    const peerId = PEER_ID_PREFIX + code;
    // Once the initial DataConnection opens, transient peer errors (e.g.,
    // temporary signaling-server hiccups, failed peer-discovery on a stale
    // peer-id) must NOT destroy the Peer — the guest adapter needs it alive to
    // call `peer.connect(hostPeerId)` during auto-reconnect. Only fatal Peer
    // errors tear down the Peer; everything else is logged and ignored.
    let opened = false;

    peer.on("open", () => {
      console.log("[P2P Guest] registered on signaling server, connecting to:", peerId);
      const conn = peer.connect(peerId);

      const timeout = setTimeout(() => {
        reject(new Error("Connection timed out. Check the room code and try again."));
        peer.destroy();
      }, 30_000);

      conn.on("open", () => {
        clearTimeout(timeout);
        opened = true;
        resolve({
          conn,
          peer,
          closeConn: () => {
            try { conn.close(); } catch { /* best-effort */ }
          },
          destroyPeer: () => {
            try { peer.destroy(); } catch { /* best-effort */ }
          },
        });
      });

      conn.on("error", (err) => {
        if (opened) {
          // Post-open conn errors are surfaced to the PeerSession layer (via
          // its own `conn.on("error")` in `peer.ts`) and handled as a session
          // disconnect that triggers auto-reconnect. Do NOT tear down the Peer
          // here — that would kill the auto-reconnect channel.
          console.warn("[P2P Guest] post-open connection error (non-fatal):", err);
          return;
        }
        clearTimeout(timeout);
        reject(new Error(`Connection error: ${err.message}`));
        peer.destroy();
      });
    });

    // PeerJS emits connection failures on the peer, not the conn (issue #1281).
    // Mirror the host's classifier: post-open, only fatal types destroy the
    // Peer. The same fatal set applies on both sides of the signaling server.
    peer.on("error", (err: Error & { type?: string }) => {
      const fatal = err.type === "browser-incompatible"
        || err.type === "invalid-id"
        || err.type === "invalid-key"
        || err.type === "unavailable-id"
        || err.type === "ssl-unavailable"
        || err.type === "server-error"
        || err.type === "socket-error"
        || err.type === "socket-closed";
      if (!opened) {
        // Pre-open: any peer error means the initial connect failed — reject.
        reject(new Error(`Failed to connect: ${err.message}`));
        try { peer.destroy(); } catch { /* best-effort */ }
        return;
      }
      if (fatal) {
        console.error("[P2P Guest] fatal Peer error, destroying:", err);
        try { peer.destroy(); } catch { /* best-effort */ }
      } else {
        console.warn("[P2P Guest] non-fatal Peer error (Peer kept alive for reconnect):", err);
      }
    });
  });
}
