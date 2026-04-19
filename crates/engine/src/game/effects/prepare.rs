use crate::types::ability::{Effect, EffectError, EffectKind, ResolvedAbility, TargetRef};
use crate::types::card::LayoutKind;
use crate::types::events::GameEvent;
use crate::types::game_state::GameState;
use crate::types::identifiers::ObjectId;

use crate::game::game_object::PreparedState;

// KNOWN_LIMITATION: The reminder text for Prepare says a copy of the
// prepare-spell "appears in exile" while the creature is prepared. This design
// does NOT materialize that copy as a GameObject. The cast-offer is produced
// at priority time by scanning battlefield creatures whose `prepared.is_some()`
// and whose printed `CardLayout::Prepare(_, b)` supplies face `b`. As a result,
// exile-event triggers and "going-to-exile" replacement effects (Rest in
// Peace, Leyline of the Void, Containment Priest) will NOT observe the copy.
// Acceptable for the SOS-era cards — no card in the set interacts with
// prepare-copies through those hooks — and aligns with CR 707.10a which
// already makes spell copies cease to exist off-stack. If a future card
// requires the copy to be a first-class exile GameObject, materialization can
// be retrofitted around the existing offer scan without touching the resolver
// layer.

/// Extract object targets from `ability.targets`, or fall back to `last_created_token_ids`
/// for `TargetFilter::LastCreated`. Mirrors the pattern used by `suspect::resolve`.
fn resolve_object_targets(state: &GameState, ability: &ResolvedAbility) -> Vec<ObjectId> {
    use crate::types::ability::TargetFilter;
    let filter = match &ability.effect {
        Effect::BecomePrepared { target } | Effect::BecomeUnprepared { target } => target,
        _ => return Vec::new(),
    };
    if matches!(filter, TargetFilter::LastCreated) {
        return state.last_created_token_ids.clone();
    }
    ability
        .targets
        .iter()
        .filter_map(|t| match t {
            TargetRef::Object(id) => Some(*id),
            _ => None,
        })
        .collect()
}

/// Returns true if the given permanent has a printed `CardLayout::Prepare(_, _)`
/// — i.e., is eligible to become prepared. Biblioplex-style "target creature
/// becomes prepared" effects no-op on creatures without a prepare face per the
/// reminder text: "Only creatures with prepare spells can become prepared."
fn has_prepare_face(state: &GameState, object_id: ObjectId) -> bool {
    let Some(obj) = state.objects.get(&object_id) else {
        return false;
    };
    // The printed-cards loader populates `back_face.layout_kind` with
    // `LayoutKind::Prepare` for cards whose printed `CardLayout::Prepare(_, _)`
    // supplies the prepare-spell face. Biblioplex-style "target creature
    // becomes prepared" no-ops on creatures lacking this face.
    obj.back_face
        .as_ref()
        .is_some_and(|b| matches!(b.layout_kind, Some(LayoutKind::Prepare)))
}

/// CR 702.xxx: Prepare (Strixhaven) — resolver for `Effect::BecomePrepared`.
///
/// Idempotent: no-op (and no event emitted) if the target is already prepared
/// or if the target lacks a prepare face (Biblioplex gate). Otherwise sets
/// `prepared = Some(PreparedState)` and emits `BecamePrepared`. Assign when
/// WotC publishes SOS CR update.
pub fn resolve_become_prepared(
    state: &mut GameState,
    ability: &ResolvedAbility,
    events: &mut Vec<GameEvent>,
) -> Result<(), EffectError> {
    let target_ids = resolve_object_targets(state, ability);
    for object_id in target_ids {
        // Biblioplex gate — only creatures with prepare spells can become prepared.
        if !has_prepare_face(state, object_id) {
            continue;
        }
        let Some(obj) = state.objects.get_mut(&object_id) else {
            continue;
        };
        // Idempotency: no-op if already prepared.
        if obj.prepared.is_some() {
            continue;
        }
        obj.prepared = Some(PreparedState);
        events.push(GameEvent::BecamePrepared { object_id });
    }
    events.push(GameEvent::EffectResolved {
        kind: EffectKind::BecomePrepared,
        source_id: ability.source_id,
    });
    Ok(())
}

/// CR 702.xxx: Prepare (Strixhaven) — resolver for `Effect::BecomeUnprepared`.
///
/// Idempotent: no-op (and no event emitted) if the target is not prepared.
/// Otherwise clears `prepared` and emits `BecameUnprepared`. Single authority
/// for the "Doing so unprepares it." consumption — callers must not inspect
/// the field directly. Assign when WotC publishes SOS CR update.
pub fn resolve_become_unprepared(
    state: &mut GameState,
    ability: &ResolvedAbility,
    events: &mut Vec<GameEvent>,
) -> Result<(), EffectError> {
    let target_ids = resolve_object_targets(state, ability);
    for object_id in target_ids {
        let Some(obj) = state.objects.get_mut(&object_id) else {
            continue;
        };
        if obj.prepared.is_none() {
            continue;
        }
        obj.prepared = None;
        events.push(GameEvent::BecameUnprepared { object_id });
    }
    events.push(GameEvent::EffectResolved {
        kind: EffectKind::BecomeUnprepared,
        source_id: ability.source_id,
    });
    Ok(())
}

/// Direct-call variant used by `GameAction::CastPreparedCopy` handling — flips
/// `prepared` to None on a specific object, emitting the event only when the
/// toggle actually fires. Centralizes the "cast-time unprepare" rule so the
/// action handler doesn't inspect the field directly (single-authority).
pub fn unprepare_object(state: &mut GameState, object_id: ObjectId, events: &mut Vec<GameEvent>) {
    let Some(obj) = state.objects.get_mut(&object_id) else {
        return;
    };
    if obj.prepared.is_none() {
        return;
    }
    obj.prepared = None;
    events.push(GameEvent::BecameUnprepared { object_id });
}

/// CR 702.xxx + CR 707.10f: Build a token spell-copy on the stack from the
/// prepare-spell face (face `b`) of `source_id`'s printed card. The resulting
/// stack entry mirrors the `copy_spell` effect's construction — a fresh
/// ObjectId, `is_token = true`, `CastingVariant::Normal`, controller = acting
/// player. The source creature is unprepared at cast time (reminder: "Doing
/// so unprepares it."), not on resolution — so counter-the-copy leaves the
/// source permanently unprepared.
///
/// Returns Ok(copy_id) on success. Returns Err if the source is not prepared,
/// lacks a prepare face, or doesn't exist. Assign when WotC publishes SOS CR
/// update.
pub fn cast_prepared_copy(
    state: &mut GameState,
    source_id: ObjectId,
    controller: crate::types::player::PlayerId,
    events: &mut Vec<GameEvent>,
) -> Result<ObjectId, String> {
    use crate::types::ability::{ResolvedAbility, TargetRef};
    use crate::types::game_state::{CastingVariant, StackEntry, StackEntryKind};
    use crate::types::zones::Zone;

    let (src_clone, card_id) = {
        let Some(src_obj) = state.objects.get(&source_id) else {
            return Err(format!("source {source_id:?} not found"));
        };
        if src_obj.prepared.is_none() {
            return Err("source is not prepared".to_string());
        }
        (src_obj.clone(), src_obj.card_id)
    };
    let Some(back) = src_clone.back_face.clone() else {
        return Err("source has no prepare face".to_string());
    };
    if !matches!(back.layout_kind, Some(LayoutKind::Prepare)) {
        return Err("source back_face is not a Prepare face".to_string());
    }
    // Select the first ability on face_b as the spell ability. SOS prepare
    // spells each have a single spell ability (Sorcery-type); more complex
    // multi-ability prepare faces are out of scope.
    let ability_def = back
        .abilities
        .first()
        .cloned()
        .ok_or_else(|| "prepare face has no spell ability".to_string())?;

    // Allocate a new object id for the copy.
    let copy_id = ObjectId(state.next_object_id);
    state.next_object_id += 1;

    // Build a GameObject for the token copy — clone core characteristics from
    // back_face so zone transitions and filter predicates see the correct
    // face. Name from face_b, zone = Stack, is_token = true.
    let mut copy_obj = src_clone;
    copy_obj.id = copy_id;
    copy_obj.name = back.name.clone();
    copy_obj.power = back.power;
    copy_obj.toughness = back.toughness;
    copy_obj.loyalty = back.loyalty;
    copy_obj.defense = back.defense;
    copy_obj.card_types = back.card_types.clone();
    copy_obj.mana_cost = back.mana_cost.clone();
    copy_obj.keywords = back.keywords.clone();
    copy_obj.abilities = back.abilities.clone();
    copy_obj.color = back.color.clone();
    copy_obj.printed_ref = back.printed_ref.clone();
    copy_obj.controller = controller;
    copy_obj.owner = controller;
    copy_obj.zone = Zone::Stack;
    copy_obj.is_token = true;
    // CR 702.xxx: the copy is a distinct object — clear any per-permanent
    // state carried over from the source's creature face.
    copy_obj.tapped = false;
    copy_obj.prepared = None;
    copy_obj.back_face = None;
    state.objects.insert(copy_id, copy_obj);

    // Build a resolved ability from face_b's ability definition with no targets
    // (targets are established by later target selection for legal targets).
    let resolved = ResolvedAbility::new(
        *ability_def.effect.clone(),
        Vec::<TargetRef>::new(),
        copy_id,
        controller,
    );

    state.stack.push(StackEntry {
        id: copy_id,
        source_id: copy_id,
        controller,
        kind: StackEntryKind::Spell {
            card_id,
            ability: Some(resolved),
            casting_variant: CastingVariant::Normal,
            actual_mana_spent: 0,
        },
    });
    events.push(crate::types::events::GameEvent::StackPushed { object_id: copy_id });

    // CR 702.xxx: "Doing so unprepares it." Unprepare-at-cast, not at resolve —
    // so countered / fizzled copies still leave the source unprepared. Single
    // authority via `unprepare_object`.
    unprepare_object(state, source_id, events);

    Ok(copy_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::zones::create_object;
    use crate::parser::oracle_effect::parse_effect;
    use crate::types::ability::TargetFilter;
    use crate::types::card_type::CoreType;
    use crate::types::identifiers::CardId;
    use crate::types::player::PlayerId;
    use crate::types::zones::Zone;

    // CR 702.xxx: Parser tests for "becomes prepared" / "becomes unprepared"
    // imperative patterns. Assign when WotC publishes SOS CR update.
    #[test]
    fn parse_target_becomes_prepared() {
        let effect = parse_effect("Target creature becomes prepared.");
        assert!(
            matches!(effect, Effect::BecomePrepared { .. }),
            "expected BecomePrepared, got {effect:?}"
        );
    }

    #[test]
    fn parse_target_becomes_unprepared() {
        let effect = parse_effect("Target creature becomes unprepared.");
        assert!(
            matches!(effect, Effect::BecomeUnprepared { .. }),
            "expected BecomeUnprepared, got {effect:?}"
        );
    }

    fn setup_creature(state: &mut GameState) -> ObjectId {
        let id = create_object(
            state,
            CardId(1),
            PlayerId(0),
            "Test Creature".to_string(),
            Zone::Battlefield,
        );
        let obj = state.objects.get_mut(&id).unwrap();
        obj.card_types.core_types.push(CoreType::Creature);
        obj.base_power = Some(2);
        obj.base_toughness = Some(2);
        obj.power = Some(2);
        obj.toughness = Some(2);
        id
    }

    #[test]
    fn become_prepared_noop_without_prepare_face() {
        // Biblioplex gate — a creature that isn't a prepare-family card must
        // not become prepared even if targeted.
        let mut state = GameState::new_two_player(42);
        let id = setup_creature(&mut state);

        let ability = ResolvedAbility::new(
            Effect::BecomePrepared {
                target: TargetFilter::Any,
            },
            vec![TargetRef::Object(id)],
            ObjectId(100),
            PlayerId(0),
        );
        let mut events = Vec::new();
        resolve_become_prepared(&mut state, &ability, &mut events).unwrap();

        let obj = state.objects.get(&id).unwrap();
        assert!(
            obj.prepared.is_none(),
            "creature without prepare face must not become prepared"
        );
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, GameEvent::BecamePrepared { .. })),
            "no BecamePrepared event on no-op"
        );
    }

    #[test]
    fn become_unprepared_is_idempotent() {
        let mut state = GameState::new_two_player(42);
        let id = setup_creature(&mut state);

        let ability = ResolvedAbility::new(
            Effect::BecomeUnprepared {
                target: TargetFilter::Any,
            },
            vec![TargetRef::Object(id)],
            ObjectId(100),
            PlayerId(0),
        );
        let mut events = Vec::new();
        resolve_become_unprepared(&mut state, &ability, &mut events).unwrap();

        assert!(
            !events
                .iter()
                .any(|e| matches!(e, GameEvent::BecameUnprepared { .. })),
            "no BecameUnprepared event when already unprepared"
        );
    }

    #[test]
    fn unprepare_object_flips_and_emits_event() {
        let mut state = GameState::new_two_player(42);
        let id = setup_creature(&mut state);
        state.objects.get_mut(&id).unwrap().prepared = Some(PreparedState);

        let mut events = Vec::new();
        unprepare_object(&mut state, id, &mut events);

        assert!(state.objects[&id].prepared.is_none());
        assert!(events
            .iter()
            .any(|e| matches!(e, GameEvent::BecameUnprepared { object_id } if *object_id == id)));

        // Idempotency — second call must not re-emit.
        let mut events2 = Vec::new();
        unprepare_object(&mut state, id, &mut events2);
        assert!(events2.is_empty());
    }

    #[test]
    fn become_prepared_idempotent_when_already_prepared() {
        // Direct assert of the idempotency branch: resolver must not re-emit
        // the event when target is already prepared.
        let mut state = GameState::new_two_player(42);
        let id = setup_creature(&mut state);
        state.objects.get_mut(&id).unwrap().prepared = Some(PreparedState);

        let ability = ResolvedAbility::new(
            Effect::BecomePrepared {
                target: TargetFilter::Any,
            },
            vec![TargetRef::Object(id)],
            ObjectId(100),
            PlayerId(0),
        );
        let mut events = Vec::new();
        resolve_become_prepared(&mut state, &ability, &mut events).unwrap();

        assert!(
            !events
                .iter()
                .any(|e| matches!(e, GameEvent::BecamePrepared { .. })),
            "no BecamePrepared event when already prepared"
        );
    }
}
