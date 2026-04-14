import { useCallback, useState } from "react";
import { motion } from "framer-motion";

import type { PlayerId, WaitingFor } from "../../adapter/types.ts";
import { useGameDispatch } from "../../hooks/useGameDispatch.ts";
import { useCanActForWaitingState } from "../../hooks/usePlayerId.ts";
import { useGameStore } from "../../stores/gameStore.ts";
import { ChoiceOverlay, ConfirmButton } from "./ChoiceOverlay.tsx";

type BattleProtectorChoice = Extract<WaitingFor, { type: "BattleProtectorChoice" }>;

/**
 * CR 310.10 + CR 704.5w + CR 704.5x: Protector choice for a battle that
 * isn't being attacked. The battle's controller picks a legal opponent as
 * the new protector; the engine emits this modal only when `candidates.len()
 * > 1` (singleton is auto-applied engine-side).
 *
 * The picker is player-based (not card-based), so we render a simple button
 * grid of opponent names drawn from `gameState.players`. The battle's name
 * (source object) is shown for context, since this prompt can fire while
 * multiple battles are in play.
 */
export function BattleProtectorModal() {
  const canActForWaitingState = useCanActForWaitingState();
  const waitingFor = useGameStore((s) => s.waitingFor);

  if (waitingFor?.type !== "BattleProtectorChoice") return null;
  if (!canActForWaitingState) return null;

  return <BattleProtectorContent data={waitingFor.data} />;
}

function BattleProtectorContent({ data }: { data: BattleProtectorChoice["data"] }) {
  const dispatch = useGameDispatch();
  const battleName = useGameStore(
    (s) => s.gameState?.objects[data.battle_id]?.name ?? "Battle",
  );
  const [selected, setSelected] = useState<PlayerId | null>(null);

  const handleConfirm = useCallback(() => {
    if (selected != null) {
      dispatch({ type: "ChooseBattleProtector", data: { protector: selected } });
    }
  }, [dispatch, selected]);

  return (
    <ChoiceOverlay
      title="Choose a Protector"
      subtitle={`${battleName} needs a new protector. Choose which opponent will defend it.`}
      widthClassName="w-fit max-w-full"
      maxWidthClassName="max-w-3xl"
      footer={<ConfirmButton onClick={handleConfirm} disabled={selected == null} />}
    >
      <div className="mx-auto mb-6 flex w-fit max-w-3xl flex-wrap items-center justify-center gap-3 sm:mb-10">
        {data.candidates.map((candidateId, index) => {
          const isSelected = selected === candidateId;
          return (
            <motion.button
              key={candidateId}
              className={`min-h-11 rounded-lg border-2 px-4 py-3 text-sm font-semibold transition sm:px-5 sm:text-base ${
                isSelected
                  ? "border-emerald-400 bg-emerald-500/30 text-white"
                  : "border-gray-600 bg-gray-800/80 text-gray-300 hover:border-gray-400 hover:text-white"
              }`}
              initial={{ opacity: 0, y: 20, scale: 0.95 }}
              animate={{ opacity: 1, y: 0, scale: 1 }}
              transition={{ delay: 0.05 + index * 0.03, duration: 0.25 }}
              whileHover={{ scale: 1.05 }}
              onClick={() => setSelected(isSelected ? null : candidateId)}
            >
              {`Player ${candidateId + 1}`}
            </motion.button>
          );
        })}
      </div>
    </ChoiceOverlay>
  );
}
