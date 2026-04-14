import type { GameAction, GameEvent, GameState } from "../adapter/types";

export type P2PMessage =
  | { type: "guest_deck"; deckData: unknown }
  | {
      type: "game_setup";
      assignedPlayerId: number;
      playerToken: string;
      state: GameState;
      events: GameEvent[];
      legalActions: GameAction[];
      autoPassRecommended?: boolean;
    }
  | { type: "action"; senderPlayerId: number; action: GameAction }
  | {
      type: "state_update";
      state: GameState;
      events: GameEvent[];
      legalActions: GameAction[];
      autoPassRecommended?: boolean;
    }
  | { type: "action_rejected"; reason: string }
  | { type: "ping"; timestamp: number }
  | { type: "pong"; timestamp: number }
  | { type: "disconnect"; reason: string }
  | { type: "emote"; emote: string }
  | { type: "concede" }
  // Reconnect: guest presents prior token; host accepts (with fresh state) or rejects.
  | { type: "reconnect"; playerToken: string }
  | {
      type: "reconnect_ack";
      assignedPlayerId: number;
      state: GameState;
      legalActions: GameAction[];
      autoPassRecommended: boolean;
    }
  | { type: "reconnect_rejected"; reason: string }
  // Kick / forced removal (host → target).
  | { type: "kick"; reason: string }
  // Lifecycle broadcasts (host → all remaining peers).
  | { type: "player_kicked"; playerId: number; reason: string }
  // Host chose "continue without them" OR guest self-conceded mid-game. Wire
  // variant kept distinct from `player_kicked` so clients can render correctly
  // (kick = host forcibly removed; conceded = player left or was continued past).
  | { type: "player_conceded"; playerId: number; reason: string }
  | { type: "player_disconnected"; playerId: number }
  | { type: "player_reconnected"; playerId: number }
  | { type: "game_paused"; reason: string }
  | { type: "game_resumed" }
  // Pre-game lobby progress (host → all peers in the lobby).
  | { type: "lobby_progress"; joined: number; total: number };

const VALID_TYPES = new Set([
  "guest_deck",
  "game_setup",
  "action",
  "state_update",
  "action_rejected",
  "ping",
  "pong",
  "disconnect",
  "emote",
  "concede",
  "reconnect",
  "reconnect_ack",
  "reconnect_rejected",
  "kick",
  "player_kicked",
  "player_conceded",
  "player_disconnected",
  "player_reconnected",
  "game_paused",
  "game_resumed",
  "lobby_progress",
]);

/** Validate an already-parsed object as a P2PMessage. Throws on malformed data. */
export function validateMessage(raw: unknown): P2PMessage {
  if (typeof raw !== "object" || raw === null || !("type" in raw)) {
    throw new Error("Invalid message: missing type field");
  }
  const msg = raw as { type: string };
  if (!VALID_TYPES.has(msg.type)) {
    throw new Error(`Invalid message type: ${msg.type}`);
  }
  return raw as P2PMessage;
}
