import { useMemo, useCallback } from "react";

import type { GameAction, ObjectId } from "../../adapter/types.ts";
import { useCardImage } from "../../hooks/useCardImage.ts";
import { usePlayerId } from "../../hooks/usePlayerId.ts";
import { useGameStore } from "../../stores/gameStore.ts";
import { useUiStore } from "../../stores/uiStore.ts";
import { dispatchAction } from "../../game/dispatch.ts";

interface ZoneHandProps {
  zone: "exile" | "graveyard";
}

const ZONE_LABELS: Record<string, string> = {
  exile: "Exile",
  graveyard: "Graveyard",
};

export function ZoneHand({ zone }: ZoneHandProps) {
  const playerId = usePlayerId();
  const objects = useGameStore((s) => s.gameState?.objects);
  const legalActions = useGameStore((s) => s.legalActions);
  const setPendingAbilityChoice = useUiStore((s) => s.setPendingAbilityChoice);
  const inspectObject = useUiStore((s) => s.inspectObject);

  const graveyard = useGameStore((s) => s.gameState?.players[playerId]?.graveyard);
  const exile = useGameStore((s) => s.gameState?.exile);

  const zoneObjectIds = useMemo(() => {
    if (zone === "graveyard") {
      return graveyard ?? [];
    }
    if (!exile || !objects) return [];
    return exile.filter((id) => {
      const obj = objects[id];
      return obj?.owner === playerId;
    });
  }, [zone, graveyard, exile, objects, playerId]);

  // Build sets of castable object_ids and activatable object_ids from legal actions
  // (same pattern as PlayerHand.playableObjectIds)
  const { castableObjectIds, activatableObjectIds } = useMemo(() => {
    const castable = new Set<number>();
    const activatable = new Set<number>();
    for (const action of legalActions) {
      if (action.type === "PlayLand" || action.type === "CastSpell") {
        castable.add(Number((action as Extract<GameAction, { type: "PlayLand" | "CastSpell" }>).data.object_id));
      }
      if (action.type === "ActivateAbility") {
        activatable.add(Number(action.data.source_id));
      }
    }
    return { castableObjectIds: castable, activatableObjectIds: activatable };
  }, [legalActions]);

  // Filter zone objects to only castable/activatable ones, excluding face-down cards
  const castableObjects = useMemo(() => {
    if (!objects) return [];
    return zoneObjectIds
      .map((id) => objects[id])
      .filter(
        (obj) =>
          obj &&
          !obj.face_down &&
          (castableObjectIds.has(Number(obj.id)) ||
            activatableObjectIds.has(Number(obj.id))),
      );
  }, [zoneObjectIds, objects, castableObjectIds, activatableObjectIds]);

  const playCard = useCallback(
    (objectId: number) => {
      if (!objects) return;
      const obj = objects[objectId];
      if (!obj) return;

      const castAction = legalActions.find(
        (a) =>
          (a.type === "PlayLand" || a.type === "CastSpell") &&
          Number((a as Extract<GameAction, { type: "PlayLand" | "CastSpell" }>).data.object_id) === objectId,
      );
      const abilityActions = legalActions.filter(
        (a) => a.type === "ActivateAbility" && Number(a.data.source_id) === objectId,
      );

      const allActions: GameAction[] = [];
      if (castAction) allActions.push(castAction);
      allActions.push(...abilityActions);

      if (allActions.length === 0) return;
      inspectObject(null);
      if (allActions.length === 1) {
        dispatchAction(allActions[0]);
      } else {
        setPendingAbilityChoice({ objectId: objectId as ObjectId, actions: allActions });
      }
    },
    [objects, legalActions, inspectObject, setPendingAbilityChoice],
  );

  if (castableObjects.length === 0) return null;

  return (
    <div className="flex flex-row items-end">
      {castableObjects.map((obj, i) => (
        <ZoneHandCard
          key={obj.id}
          objectId={obj.id}
          cardName={obj.name}
          zone={zone}
          index={i}
          onClick={playCard}
          onMouseEnter={inspectObject}
          onMouseLeave={() => inspectObject(null)}
        />
      ))}
    </div>
  );
}

/** Horizontal overlap: first card at 0, subsequent cards overlap by 60% of card width */
const ZONE_HAND_OVERLAP = "calc(var(--card-w) * -0.6)";

interface ZoneHandCardProps {
  objectId: number;
  cardName: string;
  zone: "exile" | "graveyard";
  index: number;
  onClick: (objectId: number) => void;
  onMouseEnter: (objectId: number | null) => void;
  onMouseLeave: () => void;
}

function ZoneHandCard({ objectId, cardName, zone, index, onClick, onMouseEnter, onMouseLeave }: ZoneHandCardProps) {
  const { src } = useCardImage(cardName, { size: "normal" });

  return (
    <button
      data-card-hover
      onClick={() => onClick(objectId)}
      onMouseEnter={() => onMouseEnter(objectId)}
      onMouseLeave={onMouseLeave}
      className="group relative cursor-pointer transition-transform hover:z-10 hover:scale-105"
      title={`Cast from ${ZONE_LABELS[zone]}: ${cardName}`}
      style={{
        width: "var(--card-w)",
        height: "var(--card-h)",
        marginLeft: index === 0 ? 0 : ZONE_HAND_OVERLAP,
        zIndex: index,
      }}
    >
      {/* Card image with purple border */}
      <div className="relative h-full w-full overflow-hidden rounded-lg border border-purple-400/60 shadow-md">
        {src ? (
          <img
            src={src}
            alt={cardName}
            className="h-full w-full object-cover"
            draggable={false}
          />
        ) : (
          <div className="h-full w-full bg-gray-700" />
        )}

        {/* Purple translucent overlay (Arena-style) */}
        <div className="absolute inset-0 bg-purple-600/30 transition-colors group-hover:bg-purple-600/10" />
      </div>

      {/* Zone badge */}
      <div className="absolute -top-1 left-1/2 -translate-x-1/2 z-10 rounded-sm bg-purple-700 px-1.5 py-px text-[8px] font-bold text-purple-100 shadow">
        {ZONE_LABELS[zone]}
      </div>

      {/* Castable glow ring */}
      <div className="absolute inset-0 rounded-lg ring-2 ring-purple-400/70 shadow-[0_0_12px_3px_rgba(147,51,234,0.5)]" />
    </button>
  );
}
