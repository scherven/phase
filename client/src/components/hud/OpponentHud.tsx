import { useCallback, useEffect, useMemo } from "react";

import type { PlayerId } from "../../adapter/types.ts";
import { usePlayerId } from "../../hooks/usePlayerId.ts";
import { useGameStore } from "../../stores/gameStore.ts";
import { useMultiplayerStore } from "../../stores/multiplayerStore.ts";
import { usePreferencesStore } from "../../stores/preferencesStore.ts";
import { useUiStore } from "../../stores/uiStore.ts";
import { partitionByType } from "../../viewmodel/battlefieldProps.ts";
import { LifeTotal } from "../controls/LifeTotal.tsx";
import { ManaPoolSummary } from "./ManaPoolSummary.tsx";
import { StatusBadge } from "./HudBadges.tsx";
import { HudPlate } from "./HudPlate.tsx";

interface OpponentHudProps {
  opponentName?: string | null;
}

export function OpponentHud({ opponentName }: OpponentHudProps) {
  const playerId = usePlayerId();
  const focusedOpponent = useUiStore((s) => s.focusedOpponent) as PlayerId | null;
  const setFocusedOpponent = useUiStore((s) => s.setFocusedOpponent);
  const followActiveOpponent = usePreferencesStore((s) => s.followActiveOpponent);
  const setFollowActiveOpponent = usePreferencesStore((s) => s.setFollowActiveOpponent);
  const gameState = useGameStore((s) => s.gameState);

  const teamBased = gameState?.format_config?.team_based ?? false;

  const allOpponents = useMemo(() => {
    if (!gameState) return [];
    const seatOrder = gameState.seat_order ?? gameState.players.map((p) => p.id);
    return seatOrder.filter((id) => id !== playerId);
  }, [gameState, playerId]);

  const eliminated = gameState?.eliminated_players ?? [];
  const liveOpponents = allOpponents.filter((id) => !eliminated.includes(id));
  const isMultiplayer = allOpponents.length > 1;

  useEffect(() => {
    const activeOpponentId = gameState?.active_player;
    if (!followActiveOpponent || !isMultiplayer || activeOpponentId == null) {
      return;
    }
    if (!liveOpponents.includes(activeOpponentId) || focusedOpponent === activeOpponentId) {
      return;
    }
    setFocusedOpponent(activeOpponentId);
  }, [
    followActiveOpponent,
    focusedOpponent,
    gameState?.active_player,
    isMultiplayer,
    liveOpponents,
    setFocusedOpponent,
  ]);

  const waitingFor = useGameStore((s) => s.waitingFor);
  const dispatch = useGameStore((s) => s.dispatch);
  const isHumanTargetSelection =
    (waitingFor?.type === "TargetSelection" || waitingFor?.type === "TriggerTargetSelection")
    && waitingFor.data.player === playerId;
  const validPlayerTargetIds = useMemo(() => {
    if (!isHumanTargetSelection) return [] as number[];
    return (waitingFor.data.selection?.current_legal_targets ?? [])
      .filter((target): target is { Player: number } => "Player" in target)
      .map((target) => target.Player);
  }, [isHumanTargetSelection, waitingFor]);

  const handlePlayerTarget = useCallback(
    (targetPlayerId: number) => {
      dispatch({ type: "ChooseTarget", data: { target: { Player: targetPlayerId } } });
    },
    [dispatch],
  );

  const disconnectedPlayers = useMultiplayerStore((s) => s.disconnectedPlayers);
  const connectionStatus = useMultiplayerStore((s) => s.connectionStatus);
  const isOnline = connectionStatus !== "disconnected";

  if (!isMultiplayer) {
    // 1v1: single opponent pill (existing design)
    const opponentId = allOpponents[0] ?? (playerId === 0 ? 1 : 0);
    const isOpponentTurn = gameState?.active_player === opponentId;
    const isValidTarget = validPlayerTargetIds.includes(opponentId);
    const opponentCompanion = gameState?.players[opponentId]?.companion;
    const opponentSpeed = gameState?.players[opponentId]?.speed ?? 0;
    const isDisconnected = isOnline && disconnectedPlayers.has(opponentId);
    const label = opponentName ?? `Opp ${opponentId + 1}`;

    const hudTone = isValidTarget ? "cyan" : isOpponentTurn ? "rose" : "neutral";

    return (
      <div data-player-hud={String(opponentId)} className="relative flex items-center py-1">
        <HudPlate
          label={label}
          tone={hudTone}
          onClick={isValidTarget ? () => handlePlayerTarget(opponentId) : undefined}
          trailing={(
            <>
              {opponentSpeed > 0 ? <StatusBadge label="Speed" value={opponentSpeed} tone={opponentSpeed >= 4 ? "amber" : "neutral"} /> : null}
              {opponentCompanion ? <StatusBadge label="Companion" /> : null}
              {isOnline ? <ConnectionDotInline disconnected={isDisconnected} /> : null}
            </>
          )}
        >
          <div className="flex min-w-0 items-center gap-2">
            <LifeTotal playerId={opponentId} size="lg" hideLabel />
            <ManaPoolSummary playerId={opponentId} />
          </div>
        </HudPlate>
      </div>
    );
  }

  // Multiplayer: tabbed opponent selector
  const focusedId = focusedOpponent ?? liveOpponents[0];

  return (
    <div className="flex items-center justify-center gap-2 px-2 py-1">
      {allOpponents.map((opId) => (
        <OpponentTab
          key={opId}
          playerId={opId}
          isFocused={focusedId === opId}
          isEliminated={eliminated.includes(opId)}
          isTeammate={teamBased && isTeammate(playerId, opId)}
          isValidTarget={validPlayerTargetIds.includes(opId)}
          showMana={focusedId === opId}
          onClick={() => validPlayerTargetIds.includes(opId) ? handlePlayerTarget(opId) : setFocusedOpponent(opId)}
        />
      ))}
      <button
        type="button"
        aria-pressed={followActiveOpponent}
        onClick={() => setFollowActiveOpponent(!followActiveOpponent)}
        className={`rounded-full border px-3 py-1.5 text-[11px] font-semibold uppercase tracking-[0.16em] backdrop-blur-xl transition-all duration-200 ${
          followActiveOpponent
            ? "border-amber-300/35 bg-amber-500/18 text-amber-100 shadow-[0_10px_24px_rgba(245,158,11,0.18)]"
            : "border-white/10 bg-slate-950/62 text-slate-300 hover:border-white/20 hover:text-white"
        }`}
      >
        Follow
      </button>
    </div>
  );
}

/** 2HG team pairing: players 0+1 are team A, 2+3 are team B. */
function isTeammate(a: PlayerId, b: PlayerId): boolean {
  return Math.floor(a / 2) === Math.floor(b / 2);
}

interface OpponentTabProps {
  playerId: PlayerId;
  isFocused: boolean;
  isEliminated: boolean;
  isTeammate: boolean;
  isValidTarget: boolean;
  showMana: boolean;
  onClick: () => void;
}

function OpponentTab({ playerId, isFocused, isEliminated, isTeammate: ally, isValidTarget, showMana, onClick }: OpponentTabProps) {
  const gameState = useGameStore((s) => s.gameState);
  const isTheirTurn = gameState?.active_player === playerId;
  const player = gameState?.players[playerId];
  const isDisconnected = useMultiplayerStore((s) => s.disconnectedPlayers.has(playerId));
  const isOnline = useMultiplayerStore((s) => s.connectionStatus) !== "disconnected";

  const counts = useMemo(() => {
    if (!gameState) return { creatures: 0, lands: 0, other: 0 };
    const objects = gameState.battlefield
      .map((id) => gameState.objects[id])
      .filter(Boolean)
      .filter((obj) => obj.controller === playerId);
    const partition = partitionByType(objects);
    return {
      creatures: partition.creatures.length,
      lands: partition.lands.length,
      other: partition.support.length + partition.planeswalkers.length + partition.other.length,
    };
  }, [gameState, playerId]);

  if (!player) return null;

  const handCount = player.hand.length;
  const speed = player.speed ?? 0;

  const label = ally ? "Ally" : `Opp ${playerId + 1}`;

  const borderClass = isValidTarget
    ? "border-cyan-400/45 bg-cyan-950/45 ring-1 ring-cyan-300/45 shadow-[0_14px_28px_rgba(34,211,238,0.16)] cursor-pointer"
    : isTheirTurn
      ? "border-rose-400/45 bg-rose-950/40 ring-1 ring-rose-300/35 shadow-[0_14px_28px_rgba(244,63,94,0.16)]"
      : ally
        ? isFocused
          ? "border-emerald-400/40 bg-emerald-950/40 ring-1 ring-emerald-300/30"
          : "border-emerald-700/40 bg-slate-950/70 hover:border-emerald-400/40 hover:bg-slate-900/72"
        : isFocused
          ? "border-amber-400/40 bg-amber-950/38 ring-1 ring-amber-300/30"
          : "border-white/10 bg-slate-950/70 hover:border-white/20 hover:bg-slate-900/72";

  return (
    <button
      type="button"
      onClick={onClick}
      disabled={isEliminated}
      className={`flex items-center gap-3 rounded-[18px] border px-3 py-2 backdrop-blur-xl transition-all duration-200 ${borderClass} ${isEliminated ? "opacity-40 grayscale" : ""}`}
    >
      <div className="flex min-w-[4.5rem] flex-col items-start leading-none">
        <span className="mb-1 text-[10px] font-semibold uppercase tracking-[0.18em] text-white/48">
          {label}
        </span>
        <div className="flex items-center gap-1">
          {isTheirTurn && <span className="h-1.5 w-1.5 rounded-full bg-rose-400 animate-pulse" />}
          <span className={`text-sm font-semibold ${isTheirTurn ? "text-rose-200" : ally ? "text-emerald-200" : isFocused ? "text-amber-100" : "text-slate-100"}`}>
            {player.life}
          </span>
          {isOnline && <ConnectionDotInline disconnected={isDisconnected} />}
        </div>
      </div>

      <Stat label="Hand" value={handCount} color="text-slate-200" />
      {speed > 0 && <Stat label="Speed" value={speed} color={speed >= 4 ? "text-amber-200" : "text-slate-200"} />}
      {counts.creatures > 0 && <Stat label="Creatures" value={counts.creatures} color="text-rose-200" />}
      {counts.lands > 0 && <Stat label="Lands" value={counts.lands} color="text-emerald-200" />}
      {counts.other > 0 && <Stat label="Other" value={counts.other} color="text-cyan-200" />}

      {player.companion != null && (
        <StatusBadge label="Companion" tone={player.companion.used ? "neutral" : "amber"} />
      )}

      {showMana && <ManaPoolSummary playerId={playerId} />}

      {isEliminated && (
        <span className="rounded-full bg-red-900/60 px-2 py-1 text-[10px] font-bold uppercase tracking-[0.16em] text-red-300">Out</span>
      )}
    </button>
  );
}

function ConnectionDotInline({ disconnected }: { disconnected: boolean }) {
  return (
    <span
      className={`inline-block h-2 w-2 rounded-full ring-1 ring-white/20 ${disconnected ? "bg-red-500 animate-pulse" : "bg-emerald-400"}`}
      title={disconnected ? "Disconnected" : "Connected"}
    />
  );
}

function Stat({ label, value, color }: { label: string; value: number; color: string }) {
  return (
    <div className="flex flex-col items-start leading-none">
      <span className="mb-1 text-[9px] font-medium uppercase tracking-[0.16em] text-white/40">{label}</span>
      <span className={`text-sm font-semibold tabular-nums ${color}`}>{value}</span>
    </div>
  );
}
