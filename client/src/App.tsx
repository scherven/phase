import { lazy, Suspense, useCallback, useEffect, useState } from "react";
import { BrowserRouter, Routes, Route, useLocation } from "react-router";

import { BuildBadge } from "./components/chrome/BuildBadge";
import { HostingBanner } from "./components/chrome/HostingBanner";
import { SplashScreen } from "./components/splash/SplashScreen";
import { useFeedInitialization } from "./hooks/useFeedInitialization";
import { useHostingSession } from "./hooks/useHostingSession";
import { ensurePreload, subscribePreload } from "./startup/preloadAssets";
import { MenuPage } from "./pages/MenuPage";
import { PlayPage } from "./pages/PlayPage";
import { GamePage } from "./pages/GamePage";
import { GameSetupPage } from "./pages/GameSetupPage";

const MultiplayerPage = lazy(() => import("./pages/MultiplayerPage").then((m) => ({ default: m.MultiplayerPage })));
const DeckBuilderPage = lazy(() => import("./pages/DeckBuilderPage").then((m) => ({ default: m.DeckBuilderPage })));
const MyDecksPage = lazy(() => import("./pages/MyDecksPage").then((m) => ({ default: m.MyDecksPage })));
const CoveragePage = lazy(() => import("./pages/CoveragePage").then((m) => ({ default: m.CoveragePage })));

export function App() {
  return (
    <BrowserRouter>
      <AppContent />
    </BrowserRouter>
  );
}

function AppContent() {
  useFeedInitialization();
  useHostingSession();

  const [showSplash, setShowSplash] = useState(true);
  const [progress, setProgress] = useState(0);
  const [loadLabel, setLoadLabel] = useState("Loading...");
  const location = useLocation();

  // Run startup preload: WASM init (0–50%) → SFX preload (50–100%)
  useEffect(() => {
    if (!showSplash) return;

    const unsub = subscribePreload((p) => {
      setProgress(p.percent);
      if (p.phase === "wasm") setLoadLabel("Initializing engine...");
      else if (p.phase === "audio") setLoadLabel("Loading audio...");
      else setLoadLabel("Ready");
    });
    ensurePreload();
    return unsub;
  }, [showSplash]);

  const handleSplashComplete = useCallback(() => {
    setShowSplash(false);
  }, []);

  return (
    <div className="min-h-screen bg-gray-950 text-white">
      {showSplash && (
        <SplashScreen progress={progress} onComplete={handleSplashComplete} label={loadLabel} />
      )}
      <Suspense fallback={<div className="flex min-h-screen items-center justify-center"><div className="h-8 w-8 animate-spin rounded-full border-2 border-gray-500 border-t-white" /></div>}>
        <Routes>
          <Route path="/" element={<MenuPage />} />
          <Route path="/setup" element={<GameSetupPage />} />
          <Route path="/play" element={<PlayPage />} />
          <Route path="/multiplayer" element={<MultiplayerPage />} />
          <Route path="/my-decks" element={<MyDecksPage />} />
          <Route path="/deck-builder" element={<DeckBuilderPage />} />
          <Route path="/coverage" element={<CoveragePage />} />
          <Route path="/game/:id" element={<GamePage />} />
        </Routes>
      </Suspense>
      {!location.pathname.startsWith("/game/") && <BuildBadge />}
      <HostingBanner />
    </div>
  );
}
