import { useCallback } from "react";

import { audioManager } from "../../audio/AudioManager.ts";
import { usePlayerId } from "../../hooks/usePlayerId.ts";
import { useGameStore } from "../../stores/gameStore.ts";
import { LifeTotal } from "../controls/LifeTotal.tsx";
import { ManaPoolSummary } from "./ManaPoolSummary.tsx";
import { PhaseIndicatorLeft, PhaseIndicatorRight } from "../controls/PhaseStopBar.tsx";
import { usePreferencesStore } from "../../stores/preferencesStore.ts";

export function PlayerHud() {
  const masterMuted = usePreferencesStore((s) => s.masterMuted);
  const setMasterMuted = usePreferencesStore((s) => s.setMasterMuted);
  const playerId = usePlayerId();
  const isMyTurn = useGameStore((s) => s.gameState?.active_player === playerId);
  const speed = useGameStore((s) => s.gameState?.players[playerId]?.speed ?? 0);
  const waitingFor = useGameStore((s) => s.waitingFor);
  const dispatch = useGameStore((s) => s.dispatch);

  const isHumanTargetSelection =
    (waitingFor?.type === "TargetSelection" || waitingFor?.type === "TriggerTargetSelection")
    && waitingFor.data.player === playerId;
  const isValidTarget = isHumanTargetSelection && (waitingFor.data.selection?.current_legal_targets ?? []).some(
    (target) => "Player" in target && target.Player === playerId,
  );

  const handleTargetClick = useCallback(() => {
    if (isValidTarget) {
      dispatch({ type: "ChooseTarget", data: { target: { Player: playerId } } });
    }
  }, [isValidTarget, dispatch, playerId]);

  const pillClass = isValidTarget
    ? "bg-black/50 ring-[3px] ring-cyan-400 shadow-[0_0_20px_rgba(34,211,238,0.6),0_0_8px_rgba(34,211,238,0.4)] cursor-pointer"
    : isMyTurn
      ? "bg-black/50 ring-[3px] ring-emerald-400 shadow-[0_0_20px_rgba(52,211,153,0.5),0_0_6px_rgba(52,211,153,0.4)]"
      : "bg-black/50";

  const nameColorClass = isValidTarget
    ? "text-cyan-300"
    : isMyTurn
      ? "text-emerald-300"
      : "text-gray-300";

  const nameBgClass = isValidTarget
    ? "bg-cyan-900/80 ring-1 ring-cyan-400/50"
    : isMyTurn
      ? "bg-emerald-900/80 ring-1 ring-emerald-400/40"
      : "bg-gray-800/90 ring-1 ring-gray-600/50";

  return (
    <div
      data-player-hud={playerId}
      className="relative z-20 flex shrink-0 flex-row flex-nowrap items-center justify-center gap-1 px-1 py-0.5 lg:gap-3 lg:px-2 lg:py-1"
    >
      <PhaseIndicatorLeft />
      <div className="flex flex-col items-center">
        {/* Name badge — overlaps top of pill */}
        <div className={`z-10 -mb-1.5 flex items-center gap-1 rounded-full px-2.5 py-0.5 ${nameBgClass}`}>
          {isMyTurn && <span className="h-1.5 w-1.5 rounded-full bg-emerald-400 animate-pulse" />}
          <span className={`text-[11px] font-semibold uppercase tracking-widest lg:text-xs ${nameColorClass}`}>
            P{playerId + 1}
          </span>
        </div>
        <div
          onClick={handleTargetClick}
          className={`flex min-w-0 flex-nowrap items-center justify-center gap-0.5 rounded-full px-1.5 py-px text-[9px] transition-all duration-300 lg:gap-2 lg:px-3 lg:py-1 lg:text-xs ${pillClass}`}
        >
          <LifeTotal playerId={playerId} size="lg" hideLabel />
          {speed > 0 && (
            <span
              className={`rounded-full px-1.5 py-0.5 text-[9px] font-semibold tracking-[0.12em] lg:text-[10px] ${
                speed >= 4 ? "bg-amber-400/20 text-amber-200 ring-1 ring-amber-400/40" : "bg-white/8 text-gray-300"
              }`}
            >
              🏁 {speed}
            </span>
          )}
          <ManaPoolSummary playerId={playerId} />
          <button
            onClick={() => {
              const willUnmute = masterMuted;
              setMasterMuted(!masterMuted);
              if (willUnmute) audioManager.ensurePlayback();
            }}
            className={`rounded-full p-0.5 transition-colors hover:bg-white/10 hover:text-gray-300 lg:p-1.5 ${
              masterMuted ? "text-red-400" : "text-gray-500"
            }`}
            aria-label={masterMuted ? "Unmute audio" : "Mute audio"}
          >
            {masterMuted ? (
              <svg
                xmlns="http://www.w3.org/2000/svg"
                viewBox="0 0 20 20"
                fill="currentColor"
                className="h-3 w-3 lg:h-4 lg:w-4"
              >
                <path d="M9.547 3.062A.75.75 0 0 1 10 3.75v12.5a.75.75 0 0 1-1.264.546L5.203 13.5H2.667a.75.75 0 0 1-.7-.48A6.985 6.985 0 0 1 1.5 10c0-.85.151-1.665.429-2.42a.75.75 0 0 1 .737-.58h2.499l3.533-3.296a.75.75 0 0 1 .849-.142ZM13.28 7.22a.75.75 0 1 0-1.06 1.06L13.94 10l-1.72 1.72a.75.75 0 0 0 1.06 1.06L15 11.06l1.72 1.72a.75.75 0 1 0 1.06-1.06L16.06 10l1.72-1.72a.75.75 0 0 0-1.06-1.06L15 8.94l-1.72-1.72Z" />
              </svg>
            ) : (
              <svg
                xmlns="http://www.w3.org/2000/svg"
                viewBox="0 0 20 20"
                fill="currentColor"
                className="h-3 w-3 lg:h-4 lg:w-4"
              >
                <path d="M10 3.75a.75.75 0 0 0-1.264-.546L5.203 6.5H2.667a.75.75 0 0 0-.7.48A6.985 6.985 0 0 0 1.5 10c0 .85.151 1.665.429 2.42a.75.75 0 0 0 .737.58h2.499l3.533 3.296A.75.75 0 0 0 10 15.75V3.75ZM15.95 5.05a.75.75 0 0 0-1.06 1.06 5.5 5.5 0 0 1 0 7.78.75.75 0 0 0 1.06 1.06 7 7 0 0 0 0-9.9Z" />
                <path d="M13.829 7.172a.75.75 0 0 0-1.06 1.06 2.5 2.5 0 0 1 0 3.536.75.75 0 0 0 1.06 1.06 4 4 0 0 0 0-5.656Z" />
              </svg>
            )}
          </button>
        </div>
      </div>
      <PhaseIndicatorRight />
    </div>
  );
}
