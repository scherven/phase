import { AnimatePresence, motion } from "framer-motion";
import { useCallback, useEffect } from "react";

import { usePlayerId } from "../../hooks/usePlayerId.ts";
import { useGameStore } from "../../stores/gameStore.ts";
import { useUiStore } from "../../stores/uiStore.ts";

export function TargetingOverlay() {
  const playerId = usePlayerId();
  const waitingFor = useGameStore((s) => s.waitingFor);
  const dispatch = useGameStore((s) => s.dispatch);
  const objects = useGameStore((s) => s.gameState?.objects);
  const selectedCardIds = useUiStore((s) => s.selectedCardIds);
  const clearSelectedCards = useUiStore((s) => s.clearSelectedCards);

  const isTargetSelection = waitingFor?.type === "TargetSelection" || waitingFor?.type === "TriggerTargetSelection";
  const isCopyTargetChoice = waitingFor?.type === "CopyTargetChoice";
  const isExploreChoice = waitingFor?.type === "ExploreChoice";
  const isTapCreatureChoice = waitingFor?.type === "TapCreaturesForManaAbility";
  const targetSlots = isTargetSelection ? waitingFor.data.target_slots : [];
  const selection = isTargetSelection ? waitingFor.data.selection : null;
  const currentTargetSlot = selection?.current_slot ?? 0;
  const activeSlot = targetSlots[currentTargetSlot];
  const isOptionalCurrentSlot = activeSlot?.optional === true;

  // Derive context for the targeting prompt
  const sourceId = waitingFor?.type === "TriggerTargetSelection"
    ? waitingFor.data.source_id
    : waitingFor?.type === "TargetSelection"
      ? waitingFor.data.pending_cast?.object_id
      : waitingFor?.type === "ExploreChoice"
        ? waitingFor.data.source_id
      : waitingFor?.type === "TapCreaturesForManaAbility"
        ? (waitingFor.data.pending_mana_ability as { source_id?: number } | undefined)?.source_id
      : undefined;
  const sourceName = sourceId != null ? objects?.[sourceId]?.name : undefined;
  const triggerDescription = waitingFor?.type === "TriggerTargetSelection"
    ? waitingFor.data.description
    : undefined;

  const handleCancel = useCallback(() => {
    dispatch({ type: "CancelCast" });
  }, [dispatch]);

  const handleSkip = useCallback(() => {
    dispatch({ type: "ChooseTarget", data: { target: null } });
  }, [dispatch]);

  const handleConfirmTap = useCallback(() => {
    dispatch({ type: "SelectCards", data: { cards: selectedCardIds } });
  }, [dispatch, selectedCardIds]);

  useEffect(() => {
    if (!isTapCreatureChoice) {
      clearSelectedCards();
      return;
    }
    clearSelectedCards();
    return () => clearSelectedCards();
  }, [clearSelectedCards, isTapCreatureChoice]);

  if (!isTargetSelection && !isCopyTargetChoice && !isExploreChoice && !isTapCreatureChoice) return null;

  // Only show targeting UI for the human player
  if (waitingFor.data.player !== playerId) return null;

  return (
    <AnimatePresence>
      <motion.div
        className="pointer-events-none fixed inset-0 z-40"
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        exit={{ opacity: 0 }}
        transition={{ duration: 0.2 }}
      >
        {/* Semi-transparent overlay (click-through so board cards remain clickable) */}
        <div className="absolute inset-0 bg-black/30" />

        {/* Instruction text */}
        <div className="absolute left-0 right-0 top-4 flex flex-col items-center gap-1">
          {sourceName && (
            <div className="rounded-md bg-gray-800/90 px-4 py-1 text-sm font-medium text-amber-300 shadow">
              {sourceName}
            </div>
          )}
            <div className="rounded-lg bg-gray-900/90 px-6 py-2 text-lg font-semibold text-cyan-400 shadow-lg">
              {isCopyTargetChoice
                ? "Choose a permanent to copy"
                : isExploreChoice
                  ? "Choose which creature explores next"
                : isTapCreatureChoice
                  ? `Tap ${waitingFor.data.count} untapped creature${waitingFor.data.count > 1 ? "s" : ""}`
                : targetSlots.length > 1
                  ? `Choose target ${Math.min(currentTargetSlot + 1, targetSlots.length)} of ${targetSlots.length}`
                  : "Choose a target"}
            </div>
          {triggerDescription && (
            <div className="max-w-md rounded-md bg-gray-800/90 px-4 py-1 text-center text-xs text-gray-300 shadow">
              {triggerDescription}
            </div>
          )}
        </div>

        {/* Player targets are handled by PlayerHud/OpponentHud glow + click */}

        <div className="pointer-events-auto absolute bottom-6 left-0 right-0 flex justify-center gap-4">
          {waitingFor.type === "TargetSelection" && (
            <button
              onClick={handleCancel}
              className="rounded-lg bg-gray-700 px-6 py-2 font-semibold text-gray-200 shadow-lg transition hover:bg-gray-600"
            >
              Cancel
            </button>
          )}
          {isTapCreatureChoice && (
            <button
              onClick={handleConfirmTap}
              disabled={selectedCardIds.length !== waitingFor.data.count}
              className="rounded-lg bg-emerald-700 px-6 py-2 font-semibold text-gray-100 shadow-lg transition hover:bg-emerald-600 disabled:cursor-not-allowed disabled:bg-gray-700 disabled:text-gray-400"
            >
              Confirm Tap ({selectedCardIds.length}/{waitingFor.data.count})
            </button>
          )}
          {isOptionalCurrentSlot && (
            <button
              onClick={handleSkip}
              className="rounded-lg bg-amber-700 px-6 py-2 font-semibold text-gray-100 shadow-lg transition hover:bg-amber-600"
            >
              Skip
            </button>
          )}
        </div>
      </motion.div>
    </AnimatePresence>
  );
}
