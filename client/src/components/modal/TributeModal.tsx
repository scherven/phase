import type { WaitingFor } from "../../adapter/types.ts";
import { useGameDispatch } from "../../hooks/useGameDispatch.ts";
import { useCanActForWaitingState } from "../../hooks/usePlayerId.ts";
import { useGameStore } from "../../stores/gameStore.ts";
import { ChoiceModal } from "./ChoiceModal.tsx";

type TributeChoice = Extract<WaitingFor, { type: "TributeChoice" }>;

/**
 * CR 702.104a: Tribute pay/decline prompt. The controller's opponent-pick
 * (phase 1) is handled upstream by the generic `NamedChoice { Opponent }`
 * flow; this modal covers phase 2 — the chosen opponent decides whether to
 * place N +1/+1 counters on the entering creature. The engine persists the
 * outcome as `ChosenAttribute::TributeOutcome` so the companion
 * "if tribute wasn't paid" trigger (CR 702.104b) reads the decision.
 *
 * Reuses `GameAction::DecideOptionalEffect { accept }` per the engine
 * contract — no new action variant needed.
 */
export function TributeModal() {
  const canActForWaitingState = useCanActForWaitingState();
  const waitingFor = useGameStore((s) => s.waitingFor);
  const dispatch = useGameDispatch();
  const sourceName = useGameStore(
    (s) =>
      waitingFor?.type === "TributeChoice"
        ? s.gameState?.objects[waitingFor.data.source_id]?.name ?? "Tribute creature"
        : null,
  );

  if (waitingFor?.type !== "TributeChoice") return null;
  if (!canActForWaitingState) return null;

  const data = waitingFor.data as TributeChoice["data"];
  const counters = data.count;
  const plural = counters === 1 ? "counter" : "counters";

  return (
    <ChoiceModal
      title={`Tribute \u2014 ${sourceName}`}
      subtitle={`Place ${counters} +1/+1 ${plural} on ${sourceName}?`}
      options={[
        {
          id: "pay",
          label: `Pay Tribute`,
          description: `Put ${counters} +1/+1 ${plural} on ${sourceName}.`,
        },
        {
          id: "decline",
          label: "Decline",
          description: "Refuse tribute. Triggers \"if tribute wasn't paid\".",
        },
      ]}
      onChoose={(id) =>
        dispatch({ type: "DecideOptionalEffect", data: { accept: id === "pay" } })
      }
    />
  );
}
