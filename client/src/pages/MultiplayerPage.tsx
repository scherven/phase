import { useCallback, useEffect, useState } from "react";
import { useNavigate } from "react-router";

import type { GameFormat } from "../adapter/types";
import { useAudioContext } from "../audio/useAudioContext";
import { ScreenChrome } from "../components/chrome/ScreenChrome";
import { HostSetup } from "../components/lobby/HostSetup";
import { LobbyView } from "../components/lobby/LobbyView";
import { WaitingScreen } from "../components/lobby/WaitingScreen";
import { MenuParticles } from "../components/menu/MenuParticles";
import { MenuShell } from "../components/menu/MenuShell";
import { MyDecks } from "../components/menu/MyDecks";
import { ACTIVE_DECK_KEY, loadActiveDeck } from "../constants/storage";
import { parseRoomCode } from "../network/connection";
import { useMultiplayerStore } from "../stores/multiplayerStore";
import { useGameStore, saveActiveGame } from "../stores/gameStore";
import type { HostSettings } from "../components/lobby/HostSetup";

type ConnectionMode = "server" | "p2p";
type MultiplayerView = "lobby" | "host-setup" | "deck-select" | "waiting";

type PendingAction =
  | { type: "host"; settings: HostSettings; connectionMode: ConnectionMode }
  | { type: "join"; code: string; password?: string; format?: GameFormat };

export function MultiplayerPage() {
  useAudioContext("lobby");
  const navigate = useNavigate();

  const hostingStatus = useMultiplayerStore((s) => s.hostingStatus);
  const hostGameCode = useMultiplayerStore((s) => s.hostGameCode);
  const hostIsPublic = useMultiplayerStore((s) => s.hostIsPublic);
  const startHosting = useMultiplayerStore((s) => s.startHosting);
  const cancelHosting = useMultiplayerStore((s) => s.cancelHosting);
  const playerSlots = useMultiplayerStore((s) => s.playerSlots);

  // If returning to this page while hosting, show waiting view immediately
  const [view, setView] = useState<MultiplayerView>(
    hostingStatus !== "idle" ? "waiting" : "lobby",
  );
  const [activeDeckName, setActiveDeckName] = useState<string | null>(null);
  const [connectionMode, setConnectionMode] = useState<ConnectionMode>("server");
  const [showSettings, setShowSettings] = useState(false);
  const [pendingAction, setPendingAction] = useState<PendingAction | null>(null);

  useEffect(() => {
    setActiveDeckName(localStorage.getItem(ACTIVE_DECK_KEY));
  }, []);

  // Reset view to lobby if hosting ends while we're on the waiting screen
  // (e.g. WebSocket error/disconnect)
  useEffect(() => {
    if (hostingStatus === "idle" && view === "waiting") {
      setView("lobby");
    }
  }, [hostingStatus, view]);

  const handleSelectDeck = (name: string) => {
    setActiveDeckName(name);
    localStorage.setItem(ACTIVE_DECK_KEY, name);
  };

  // Expand a ParsedDeck into flat name arrays for the server
  const expandDeck = useCallback(() => {
    const deck = loadActiveDeck();
    if (!deck) return null;
    const mainDeck: string[] = [];
    for (const entry of deck.main) {
      for (let i = 0; i < entry.count; i++) {
        mainDeck.push(entry.name);
      }
    }
    const sideboard: string[] = [];
    for (const entry of deck.sideboard) {
      for (let i = 0; i < entry.count; i++) {
        sideboard.push(entry.name);
      }
    }
    return { main_deck: mainDeck, sideboard };
  }, []);

  // Execute pending action after deck is selected
  const handleDeckConfirm = useCallback(() => {
    if (!pendingAction || !activeDeckName) return;

    if (pendingAction.type === "host") {
      const deck = expandDeck();
      if (!deck) return;

      if (pendingAction.connectionMode === "p2p") {
        const gameId = crypto.randomUUID();
        useGameStore.setState({ gameId });
        navigate(
          `/game/${gameId}?mode=p2p-host&match=${pendingAction.settings.matchType.toLowerCase()}`,
        );
      } else {
        startHosting(pendingAction.settings, deck);
        // Navigate to main menu — the HostingBanner takes over as the
        // hosting indicator. User can browse freely while waiting.
        // Clicking the banner returns them to /multiplayer → WaitingScreen.
        navigate("/");
      }
    } else {
      // Join flow
      const { code, password } = pendingAction;

      const p2pCode = parseRoomCode(code);
      if (p2pCode && code.trim().length === 5) {
        const gameId = crypto.randomUUID();
        useGameStore.setState({ gameId });
        navigate(`/game/${gameId}?mode=p2p-join&code=${p2pCode}`);
        return;
      }

      sessionStorage.removeItem("phase-ws-session");
      const gameId = crypto.randomUUID();
      saveActiveGame({ id: gameId, mode: "online", difficulty: "" });
      useGameStore.setState({ gameId });
      const params = new URLSearchParams({ mode: "join", code });
      if (password) {
        params.set("password", password);
      }
      navigate(`/game/${gameId}?${params.toString()}`);
    }

    setPendingAction(null);
  }, [pendingAction, activeDeckName, expandDeck, startHosting, navigate]);

  // Host setup complete → save settings, go to deck-select
  const handleHostSetupComplete = useCallback(
    (settings: HostSettings) => {
      setPendingAction({ type: "host", settings, connectionMode });
      setView("deck-select");
    },
    [connectionMode],
  );

  // Join from lobby (list or code) → save join info, go to deck-select
  const handleJoinGame = useCallback(
    (code: string, password?: string, format?: GameFormat) => {
      setPendingAction({ type: "join", code, password, format });
      setView("deck-select");
    },
    [],
  );

  const handleBack = () => {
    if (view === "waiting") {
      // Don't cancel hosting — just navigate away. The banner persists.
      navigate("/");
      return;
    }
    if (view === "deck-select") {
      setView(pendingAction?.type === "host" ? "host-setup" : "lobby");
      return;
    }
    if (view === "host-setup") {
      setView("lobby");
      return;
    }
    navigate("/");
  };

  // Derive selected format for deck filtering
  const selectedFormat: GameFormat | undefined =
    pendingAction?.type === "host"
      ? pendingAction.settings.formatConfig.format
      : pendingAction?.type === "join"
        ? pendingAction.format
        : undefined;

  const title =
    view === "lobby"
      ? "Join or host a table."
      : view === "host-setup"
        ? "Set up your table."
        : view === "deck-select"
          ? "Choose a deck."
          : "Waiting for players.";

  const description =
    view === "lobby"
      ? "Browse available tables, join by code, or host a new match."
      : view === "host-setup"
        ? "Adjust format, privacy, and timing before opening the room."
        : view === "deck-select"
          ? selectedFormat
            ? `Pick a deck for ${selectedFormat}.`
            : "Pick the deck you want to bring online."
          : "Share the code and wait for the room to fill.";

  return (
    <div className="menu-scene relative flex min-h-screen flex-col overflow-hidden">
      <MenuParticles />
      <ScreenChrome
        onBack={handleBack}
        settingsOpen={showSettings}
        onSettingsOpenChange={setShowSettings}
      />
      <div className="menu-scene__vignette" />
      <div className="menu-scene__sigil menu-scene__sigil--left" />
      <div className="menu-scene__sigil menu-scene__sigil--right" />
      <div className="menu-scene__haze" />

      <MenuShell eyebrow="Multiplayer" title={title} description={description} layout="stacked">
        {view === "lobby" && (
          <LobbyView
            onHostGame={() => { setConnectionMode("server"); setView("host-setup"); }}
            onHostP2P={() => { setConnectionMode("p2p"); setView("host-setup"); }}
            onJoinGame={handleJoinGame}
            connectionMode={connectionMode}
            onServerOffline={() => setConnectionMode("p2p")}
          />
        )}

        {view === "host-setup" && (
          <HostSetup
            onHost={handleHostSetupComplete}
            onBack={() => setView("lobby")}
            connectionMode={connectionMode}
          />
        )}

        {view === "deck-select" && (
          <MyDecks
            mode="select"
            selectedFormat={selectedFormat}
            onSelectDeck={handleSelectDeck}
            activeDeckName={activeDeckName}
            onConfirmSelection={handleDeckConfirm}
            confirmLabel={pendingAction?.type === "host" ? "Host Game" : "Join Game"}
          />
        )}

        {view === "waiting" && hostGameCode && (
          <WaitingScreen
            gameCode={hostGameCode}
            isPublic={hostIsPublic}
            onCancel={cancelHosting}
            playerSlots={playerSlots.length > 0 ? playerSlots : undefined}
            currentPlayerId="0"
            isHost
          />
        )}
      </MenuShell>
    </div>
  );
}
