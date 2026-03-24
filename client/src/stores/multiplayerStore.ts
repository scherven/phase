import { create } from "zustand";
import { persist } from "zustand/middleware";

import type { FormatConfig, GameFormat, MatchType, PlayerId } from "../adapter/types";
import { saveActiveGame, useGameStore } from "./gameStore";

type ConnectionStatus = "disconnected" | "connecting" | "connected";
type HostingStatus = "idle" | "connecting" | "waiting";

// Module-level WebSocket ref (non-serializable, lives outside store)
let hostWs: WebSocket | null = null;
// Prevents onclose from clearing session token after GameStarted
let gameStartedFired = false;
// Reconnection state for the hosting WebSocket
let hostReconnectAttempt = 0;
let hostReconnectTimer: ReturnType<typeof setTimeout> | null = null;
const HOST_MAX_RECONNECT_ATTEMPTS = 3;

export interface AiSeatConfig {
  seatIndex: number;
  difficulty: string;
  deckName: string | null;
}

export interface HostingDeck {
  main_deck: string[];
  sideboard: string[];
}

export interface HostingSettings {
  displayName: string;
  public: boolean;
  password: string;
  timerSeconds: number | null;
  formatConfig: FormatConfig;
  matchType: MatchType;
  aiSeats: AiSeatConfig[];
}

export interface PlayerSlot {
  playerId: string;
  name: string;
  isReady: boolean;
  isAi: boolean;
  aiDifficulty: string;
  deckName: string | null;
}

interface MultiplayerState {
  playerId: string;
  displayName: string;
  serverAddress: string;
  connectionStatus: ConnectionStatus;
  activePlayerId: PlayerId | null;
  opponentDisplayName: string | null;
  toastMessage: string | null;
  formatConfig: FormatConfig | null;
  playerSlots: PlayerSlot[];
  spectators: string[];
  isSpectator: boolean;
  // Hosting session (ephemeral — not persisted)
  hostGameCode: string | null;
  hostIsPublic: boolean;
  hostingStatus: HostingStatus;
  pendingGameRoute: string | null;
}

interface MultiplayerActions {
  setDisplayName: (name: string) => void;
  setServerAddress: (address: string) => void;
  setConnectionStatus: (status: ConnectionStatus) => void;
  setActivePlayerId: (id: PlayerId | null) => void;
  setOpponentDisplayName: (name: string | null) => void;
  showToast: (message: string) => void;
  clearToast: () => void;
  setFormatConfig: (config: FormatConfig | null) => void;
  setPlayerSlots: (slots: PlayerSlot[]) => void;
  toggleReady: (playerId: string) => void;
  setSpectators: (names: string[]) => void;
  setIsSpectator: (value: boolean) => void;
  // Hosting session actions
  startHosting: (settings: HostingSettings, deck: HostingDeck) => void;
  cancelHosting: () => void;
  clearPendingGameRoute: () => void;
}

export const FORMAT_DEFAULTS: Record<GameFormat, FormatConfig> = {
  Standard: {
    format: "Standard",
    starting_life: 20,
    min_players: 2,
    max_players: 2,
    deck_size: 60,
    singleton: false,
    command_zone: false,
    commander_damage_threshold: null,
    range_of_influence: null,
    team_based: false,
  },
  Pioneer: {
    format: "Pioneer",
    starting_life: 20,
    min_players: 2,
    max_players: 2,
    deck_size: 60,
    singleton: false,
    command_zone: false,
    commander_damage_threshold: null,
    range_of_influence: null,
    team_based: false,
  },
  Historic: {
    format: "Historic",
    starting_life: 20,
    min_players: 2,
    max_players: 2,
    deck_size: 60,
    singleton: false,
    command_zone: false,
    commander_damage_threshold: null,
    range_of_influence: null,
    team_based: false,
  },
  Pauper: {
    format: "Pauper",
    starting_life: 20,
    min_players: 2,
    max_players: 2,
    deck_size: 60,
    singleton: false,
    command_zone: false,
    commander_damage_threshold: null,
    range_of_influence: null,
    team_based: false,
  },
  Commander: {
    format: "Commander",
    starting_life: 40,
    min_players: 2,
    max_players: 4,
    deck_size: 100,
    singleton: true,
    command_zone: true,
    commander_damage_threshold: 21,
    range_of_influence: null,
    team_based: false,
  },
  Brawl: {
    format: "Brawl",
    starting_life: 25,
    min_players: 2,
    max_players: 2,
    deck_size: 60,
    singleton: true,
    command_zone: true,
    commander_damage_threshold: 21,
    range_of_influence: null,
    team_based: false,
  },
  HistoricBrawl: {
    format: "HistoricBrawl",
    starting_life: 25,
    min_players: 2,
    max_players: 2,
    deck_size: 60,
    singleton: true,
    command_zone: true,
    commander_damage_threshold: 21,
    range_of_influence: null,
    team_based: false,
  },
  FreeForAll: {
    format: "FreeForAll",
    starting_life: 20,
    min_players: 3,
    max_players: 6,
    deck_size: 60,
    singleton: false,
    command_zone: false,
    commander_damage_threshold: null,
    range_of_influence: null,
    team_based: false,
  },
  TwoHeadedGiant: {
    format: "TwoHeadedGiant",
    starting_life: 30,
    min_players: 4,
    max_players: 4,
    deck_size: 60,
    singleton: false,
    command_zone: false,
    commander_damage_threshold: null,
    range_of_influence: null,
    team_based: true,
  },
};

export const useMultiplayerStore = create<MultiplayerState & MultiplayerActions>()(
  persist(
    (set, get) => ({
      playerId: crypto.randomUUID(),
      displayName: "",
      serverAddress: "wss://us.phase-rs.dev/ws",
      connectionStatus: "disconnected",
      activePlayerId: null,
      opponentDisplayName: null,
      toastMessage: null,
      formatConfig: null,
      playerSlots: [],
      spectators: [],
      isSpectator: false,
      hostGameCode: null,
      hostIsPublic: false,
      hostingStatus: "idle" as HostingStatus,
      pendingGameRoute: null,

      setDisplayName: (name) => set({ displayName: name }),
      setServerAddress: (address) => set({ serverAddress: address }),
      setConnectionStatus: (status) => set({ connectionStatus: status }),
      setActivePlayerId: (id) => set({ activePlayerId: id }),
      setOpponentDisplayName: (name) => set({ opponentDisplayName: name }),
      showToast: (message) => set({ toastMessage: message }),
      clearToast: () => set({ toastMessage: null }),
      setFormatConfig: (config) => set({ formatConfig: config }),
      setPlayerSlots: (slots) => set({ playerSlots: slots }),
      toggleReady: (playerId) =>
        set((state) => ({
          playerSlots: state.playerSlots.map((slot) =>
            slot.playerId === playerId ? { ...slot, isReady: !slot.isReady } : slot,
          ),
        })),
      setSpectators: (names) => set({ spectators: names }),
      setIsSpectator: (value) => set({ isSpectator: value }),

      startHosting: (settings, deck) => {
        // Clean up any existing hosting session
        if (hostWs) {
          hostWs.close();
          hostWs = null;
        }
        if (hostReconnectTimer) {
          clearTimeout(hostReconnectTimer);
          hostReconnectTimer = null;
        }
        sessionStorage.removeItem("phase-ws-session");
        gameStartedFired = false;
        hostReconnectAttempt = 0;

        set({
          hostIsPublic: settings.public,
          hostingStatus: "connecting",
          hostGameCode: null,
          pendingGameRoute: null,
        });

        // Shared message handler for both initial and reconnect WebSockets
        const handleHostMessage = (ws: WebSocket, msg: { type: string; data?: unknown }) => {
          if (msg.type === "GameCreated") {
            const data = msg.data as { game_code: string; player_token: string };
            sessionStorage.setItem(
              "phase-ws-session",
              JSON.stringify({ gameCode: data.game_code, playerToken: data.player_token }),
            );
            // Reset reconnect counter on successful (re)connection
            hostReconnectAttempt = 0;
            set({ hostGameCode: data.game_code, hostingStatus: "waiting" });
          } else if (msg.type === "GameStarted") {
            gameStartedFired = true;
            ws.close();
            hostWs = null;
            const gameId = crypto.randomUUID();
            saveActiveGame({ id: gameId, mode: "online", difficulty: "" });
            useGameStore.setState({ gameId });
            // Reset hosting state FIRST so banner hides, then set route
            set({
              hostGameCode: null,
              hostingStatus: "idle",
              playerSlots: [],
              pendingGameRoute: `/game/${gameId}?mode=host`,
            });
          } else if (msg.type === "PlayerSlotsUpdate") {
            const data = msg.data as { slots: PlayerSlot[] };
            set({ playerSlots: data.slots });
          } else if (msg.type === "Error") {
            const data = msg.data as { message: string };
            console.error("Host error:", data.message);
            get().cancelHosting();
          }
        };

        // Attempt to reconnect the hosting WS using stored session token
        const attemptHostReconnect = () => {
          if (gameStartedFired) return;
          const raw = sessionStorage.getItem("phase-ws-session");
          if (!raw || hostReconnectAttempt >= HOST_MAX_RECONNECT_ATTEMPTS) {
            // No session to reconnect or exhausted attempts — give up
            sessionStorage.removeItem("phase-ws-session");
            set({
              hostGameCode: null,
              hostIsPublic: false,
              hostingStatus: "idle",
              playerSlots: [],
              toastMessage: "Connection to server lost.",
            });
            return;
          }

          hostReconnectAttempt++;
          const delay = Math.pow(2, hostReconnectAttempt - 1) * 1000;
          hostReconnectTimer = setTimeout(() => {
            hostReconnectTimer = null;
            if (gameStartedFired) return;

            const session = JSON.parse(raw) as { gameCode: string; playerToken: string };
            const rws = new WebSocket(get().serverAddress);
            hostWs = rws;

            rws.onopen = () => {
              rws.send(
                JSON.stringify({
                  type: "Reconnect",
                  data: {
                    game_code: session.gameCode,
                    player_token: session.playerToken,
                  },
                }),
              );
            };

            rws.onmessage = (event) => {
              const msg = JSON.parse(event.data as string) as {
                type: string;
                data?: unknown;
              };
              handleHostMessage(rws, msg);
            };

            rws.onerror = () => {
              if (!gameStartedFired) {
                hostWs = null;
                attemptHostReconnect();
              }
            };

            rws.onclose = () => {
              if (!gameStartedFired && hostWs === rws) {
                hostWs = null;
                attemptHostReconnect();
              }
            };
          }, delay);
        };

        // Wire up the initial WebSocket handlers
        const ws = new WebSocket(get().serverAddress);
        hostWs = ws;

        ws.onopen = () => {
          ws.send(
            JSON.stringify({
              type: "CreateGameWithSettings",
              data: {
                deck: { main_deck: deck.main_deck, sideboard: deck.sideboard },
                display_name: settings.displayName,
                public: settings.public,
                password: settings.password || null,
                timer_seconds: settings.timerSeconds,
                player_count: settings.formatConfig.max_players,
                match_config: { match_type: settings.matchType },
                format_config: settings.formatConfig,
                ai_seats: settings.aiSeats,
              },
            }),
          );
        };

        ws.onmessage = (event) => {
          const msg = JSON.parse(event.data as string) as {
            type: string;
            data?: unknown;
          };
          handleHostMessage(ws, msg);
        };

        ws.onerror = () => {
          if (!gameStartedFired) {
            hostWs = null;
            attemptHostReconnect();
          }
        };

        ws.onclose = () => {
          if (!gameStartedFired && hostWs === ws) {
            hostWs = null;
            attemptHostReconnect();
          }
        };
      },

      cancelHosting: () => {
        if (hostReconnectTimer) {
          clearTimeout(hostReconnectTimer);
          hostReconnectTimer = null;
        }
        if (hostWs) {
          hostWs.close();
          hostWs = null;
        }
        gameStartedFired = false;
        hostReconnectAttempt = 0;
        sessionStorage.removeItem("phase-ws-session");
        set({
          hostGameCode: null,
          hostIsPublic: false,
          hostingStatus: "idle",
          playerSlots: [],
          pendingGameRoute: null,
        });
      },

      clearPendingGameRoute: () => set({ pendingGameRoute: null }),
    }),
    {
      name: "phase-multiplayer",
      partialize: (state) => ({
        playerId: state.playerId,
        displayName: state.displayName,
        serverAddress: state.serverAddress,
      }),
    },
  ),
);
