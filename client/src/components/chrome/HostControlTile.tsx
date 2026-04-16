import { useNavigate, useLocation } from "react-router";

import {
  useMultiplayerStore,
  type PlayerSlot,
  type SeatKind,
} from "../../stores/multiplayerStore";

function seatLabel(kind: SeatKind): string {
  switch (kind.type) {
    case "HostHuman":
      return "Host";
    case "JoinedHuman":
      return "Player";
    case "WaitingHuman":
      return "Open";
    case "Ai":
      return `AI (${kind.data.difficulty})`;
  }
}

function seatColor(kind: SeatKind): string {
  switch (kind.type) {
    case "HostHuman":
      return "text-amber-400";
    case "JoinedHuman":
      return "text-emerald-400";
    case "WaitingHuman":
      return "text-slate-500";
    case "Ai":
      return "text-cyan-400";
  }
}

function SeatRow({ slot }: { slot: PlayerSlot }) {
  const isOpen = slot.kind.type === "WaitingHuman";
  return (
    <div className="flex items-center justify-between gap-2 py-0.5">
      <span className={`text-xs ${isOpen ? "italic text-slate-500" : "text-slate-300"}`}>
        {isOpen ? "Waiting…" : slot.name || `Seat ${slot.playerId}`}
      </span>
      <span className={`text-[10px] font-medium ${seatColor(slot.kind)}`}>
        {seatLabel(slot.kind)}
      </span>
    </div>
  );
}

export function HostControlTile() {
  const hostGameCode = useMultiplayerStore((s) => s.hostGameCode);
  const hostingStatus = useMultiplayerStore((s) => s.hostingStatus);
  const cancelHosting = useMultiplayerStore((s) => s.cancelHosting);
  const playerSlots = useMultiplayerStore((s) => s.playerSlots);
  const hostSession = useMultiplayerStore((s) => s.hostSession);
  const navigate = useNavigate();
  const location = useLocation();

  if (hostingStatus === "idle" || location.pathname.startsWith("/game/")) {
    return null;
  }

  const isConnecting = hostingStatus === "connecting";

  return (
    <div
      className="fixed right-3 z-30 w-56"
      style={{ top: "calc(env(titlebar-area-height, 0px) + 0.75rem)" }}
    >
      <div className="rounded-xl border border-white/10 bg-black/70 shadow-lg shadow-black/40 backdrop-blur-md">
        {/* Header */}
        <div className="flex items-center justify-between border-b border-white/5 px-3 py-2">
          <button
            type="button"
            onClick={() => navigate("/multiplayer")}
            className="flex items-center gap-2 text-xs text-slate-300 transition-colors hover:text-white"
          >
            <span className="relative flex h-2 w-2">
              <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-400 opacity-75" />
              <span className="relative inline-flex h-2 w-2 rounded-full bg-emerald-400" />
            </span>
            {isConnecting ? (
              <span className="font-medium text-slate-400">Connecting…</span>
            ) : (
              <>
                <span className="font-mono tracking-wider text-emerald-400">
                  {hostGameCode}
                </span>
                {hostSession && (
                  <span className="text-slate-500">
                    {hostSession.formatConfig.format}
                  </span>
                )}
              </>
            )}
          </button>
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              cancelHosting();
            }}
            className="text-slate-500 transition-colors hover:text-rose-400"
            aria-label="Cancel hosting"
          >
            ✕
          </button>
        </div>

        {/* Seat list — read-only in Phase 1 */}
        {playerSlots.length > 0 && (
          <div className="px-3 py-2">
            {playerSlots.map((slot) => (
              <SeatRow key={slot.playerId} slot={slot} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
