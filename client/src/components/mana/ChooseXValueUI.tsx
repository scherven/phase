import { AnimatePresence, motion } from "framer-motion";
import { useCallback, useEffect, useMemo, useState } from "react";

import { useGameStore } from "../../stores/gameStore.ts";
import { gameButtonClass } from "../ui/buttonStyles.ts";
import { ManaSymbol } from "./ManaSymbol.tsx";

/**
 * Overlay for the `WaitingFor::ChooseXValue` state.
 *
 * CR 107.1b + CR 601.2f: X must be chosen as part of determining total cost,
 * before mana is paid. The engine computes the upper bound (`max`) from the
 * player's pool + untapped free-to-tap producers; this component is a pure
 * display layer that dispatches the caster's chosen value via `ChooseX`.
 */
export function ChooseXValueUI() {
  const waitingFor = useGameStore((s) => s.waitingFor);
  const gameState = useGameStore((s) => s.gameState);
  const dispatch = useGameStore((s) => s.dispatch);

  const isChooseX = waitingFor?.type === "ChooseXValue";
  const max = isChooseX ? waitingFor.data.max : 0;
  const pendingCast = isChooseX ? waitingFor.data.pending_cast : null;

  const pendingCostShards = useMemo(() => {
    if (!pendingCast) return null;
    if (pendingCast.cost.type !== "Cost") return null;
    return pendingCast.cost.shards;
  }, [pendingCast]);

  const cardName = useMemo(() => {
    if (!gameState || !pendingCast) return null;
    return gameState.objects[pendingCast.object_id]?.name ?? null;
  }, [gameState, pendingCast]);

  const [value, setValue] = useState(0);

  useEffect(() => {
    if (isChooseX) setValue(0);
  }, [isChooseX, max]);

  const handleCommit = useCallback(() => {
    dispatch({ type: "ChooseX", data: { value } });
  }, [dispatch, value]);

  const handleCancel = useCallback(() => {
    dispatch({ type: "CancelCast" });
  }, [dispatch]);

  if (!isChooseX) return null;

  return (
    <AnimatePresence>
      <motion.div
        className="fixed inset-x-0 bottom-0 z-40 flex justify-center pb-4"
        initial={{ y: 80, opacity: 0 }}
        animate={{ y: 0, opacity: 1 }}
        exit={{ y: 80, opacity: 0 }}
        transition={{ duration: 0.25 }}
      >
        <div className="rounded-xl bg-gray-900/95 p-4 shadow-2xl ring-1 ring-gray-700 min-w-[320px] max-w-[420px]">
          <h3 className="mb-3 text-center text-sm font-semibold text-gray-300">
            Choose a value for X
            {cardName && (
              <span className="ml-1 text-gray-400">&mdash; {cardName}</span>
            )}
          </h3>

          {pendingCostShards && (
            <div className="mb-3 flex items-center justify-center gap-1.5">
              {pendingCostShards.map((shard, idx) => (
                <ManaSymbol key={idx} shard={shard} size="lg" />
              ))}
            </div>
          )}

          <div className="mb-4 px-2">
            <label className="flex items-center gap-3 text-sm text-gray-200">
              <span className="shrink-0 font-mono text-base text-cyan-300">
                X = {value}
              </span>
              <input
                type="range"
                min={0}
                max={max}
                value={value}
                onChange={(e) => setValue(Number(e.target.value))}
                className="h-2 w-full cursor-pointer appearance-none rounded-full bg-gray-700 accent-cyan-500"
                aria-label="Choose X value"
              />
              <span className="shrink-0 text-xs text-gray-500">
                max {max}
              </span>
            </label>
          </div>

          <div className="flex justify-center gap-3">
            <button
              onClick={handleCommit}
              className={gameButtonClass({ tone: "emerald", size: "md" })}
            >
              Confirm X = {value}
            </button>
            <button
              onClick={handleCancel}
              className="rounded-lg bg-gray-700 px-4 py-1.5 text-sm font-semibold text-gray-200 transition hover:bg-gray-600"
            >
              Cancel
            </button>
          </div>
        </div>
      </motion.div>
    </AnimatePresence>
  );
}
