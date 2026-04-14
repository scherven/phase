/**
 * Per-tab session token persistence for P2P games.
 *
 * Mirrors `multiplayerSession` but uses `sessionStorage` instead of
 * `localStorage` so tokens are scoped to the browser tab — closing the tab
 * clears the token, matching server-mode reconnect semantics.
 *
 * Tokens are issued by the host on `game_setup` / `reconnect_ack` and consumed
 * by the guest on auto-reconnect. Pre-game tokens (issued on lobby join but
 * before `game_setup`) are intentionally NOT persisted — a guest who drops
 * during the lobby must rejoin fresh.
 */

const STORAGE_PREFIX = "phase-p2p-session:";
const SESSION_TTL_MS = 60 * 60 * 1000;

export interface P2PSessionData {
  hostPeerId: string;
  playerToken: string;
  playerId: number;
  timestamp: number;
}

function storageKey(hostPeerId: string): string {
  return STORAGE_PREFIX + hostPeerId;
}

function isFresh(session: P2PSessionData): boolean {
  return Date.now() - session.timestamp < SESSION_TTL_MS;
}

export function saveP2PSession(
  hostPeerId: string,
  data: { playerToken: string; playerId: number },
): void {
  const session: P2PSessionData = {
    hostPeerId,
    playerToken: data.playerToken,
    playerId: data.playerId,
    timestamp: Date.now(),
  };
  sessionStorage.setItem(storageKey(hostPeerId), JSON.stringify(session));
}

export function loadP2PSession(hostPeerId: string): P2PSessionData | null {
  const raw = sessionStorage.getItem(storageKey(hostPeerId));
  if (!raw) return null;

  try {
    const session = JSON.parse(raw) as P2PSessionData;
    if (!isFresh(session)) {
      clearP2PSession(hostPeerId);
      return null;
    }
    return session;
  } catch {
    clearP2PSession(hostPeerId);
    return null;
  }
}

export function clearP2PSession(hostPeerId: string): void {
  sessionStorage.removeItem(storageKey(hostPeerId));
}
