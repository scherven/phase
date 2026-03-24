import { useNavigate, useLocation } from "react-router";

import { useMultiplayerStore } from "../../stores/multiplayerStore";

export function HostingBanner() {
  const hostGameCode = useMultiplayerStore((s) => s.hostGameCode);
  const hostingStatus = useMultiplayerStore((s) => s.hostingStatus);
  const cancelHosting = useMultiplayerStore((s) => s.cancelHosting);
  const playerSlots = useMultiplayerStore((s) => s.playerSlots);
  const navigate = useNavigate();
  const location = useLocation();

  // Hide when not hosting, on the multiplayer page (WaitingScreen visible
  // there instead), or on game pages
  if (
    hostingStatus === "idle"
    || location.pathname === "/multiplayer"
    || location.pathname.startsWith("/game/")
  ) {
    return null;
  }

  const isConnecting = hostingStatus === "connecting";
  const humanCount = playerSlots.filter((s) => !s.isAi).length;
  const totalSlots = playerSlots.length;

  return (
    <div className="fixed top-3 left-1/2 z-20 -translate-x-1/2">
      <div className="flex items-center gap-2 rounded-full border border-white/10 bg-black/18 px-3 py-1.5 text-xs shadow-lg shadow-black/30 backdrop-blur-md">
        {/* Pulsing dot */}
        <span className="relative flex h-2 w-2">
          <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-400 opacity-75" />
          <span className="relative inline-flex h-2 w-2 rounded-full bg-emerald-400" />
        </span>

        {/* Clickable label area */}
        <button
          type="button"
          onClick={() => navigate("/multiplayer")}
          className="flex items-center gap-2 text-slate-300 transition-colors hover:text-white"
        >
          {isConnecting ? (
            <span className="font-medium text-slate-400">Connecting...</span>
          ) : (
            <>
              <span className="font-medium">Hosting</span>
              <span className="font-mono tracking-wider text-emerald-400">
                {hostGameCode}
              </span>
              {totalSlots > 0 && (
                <span className="text-slate-500">
                  {humanCount}/{totalSlots}
                </span>
              )}
            </>
          )}
        </button>

        {/* Cancel button */}
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            cancelHosting();
          }}
          className="ml-0.5 text-slate-500 transition-colors hover:text-rose-400"
          aria-label="Cancel hosting"
        >
          ✕
        </button>
      </div>
    </div>
  );
}
