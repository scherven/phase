import { useCallback } from "react";

import { usePlayerId } from "../../hooks/usePlayerId.ts";
import { useGameStore } from "../../stores/gameStore.ts";
import { LifeTotal } from "../controls/LifeTotal.tsx";
import { ManaPoolSummary } from "./ManaPoolSummary.tsx";
import { PhaseIndicatorLeft, PhaseIndicatorRight } from "../controls/PhaseStopBar.tsx";
import { StatusBadge } from "./HudBadges.tsx";
import { HudPlate } from "./HudPlate.tsx";

export function PlayerHud() {
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

  const hudTone = isValidTarget ? "cyan" : isMyTurn ? "emerald" : "neutral";

  return (
    <div
      data-player-hud={playerId}
      className="relative z-20 flex shrink-0 flex-row flex-nowrap items-center justify-center gap-1.5 px-1 py-1 lg:gap-2 lg:px-2"
    >
      <PhaseIndicatorLeft />
      <HudPlate
        label={`Player ${playerId + 1}`}
        tone={hudTone}
        onClick={isValidTarget ? handleTargetClick : undefined}
        trailing={speed > 0 ? <StatusBadge label="Speed" value={speed} tone={speed >= 4 ? "amber" : "neutral"} /> : undefined}
      >
        <div className="flex min-w-0 items-center gap-2">
          <LifeTotal playerId={playerId} size="lg" hideLabel />
          <ManaPoolSummary playerId={playerId} />
        </div>
      </HudPlate>
      <PhaseIndicatorRight />
    </div>
  );
}
