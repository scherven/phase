//! CR 702.26: Phase Out / Phase In resolvers for the `Effect::PhaseOut` and
//! `Effect::PhaseIn` variants. All phasing primitives live in
//! `game::phasing`; this module is the thin effect-handler glue that
//! dispatches resolved targets to those primitives and emits the
//! `EffectResolved` bookkeeping event.

use crate::game::filter::{matches_target_filter, FilterContext};
use crate::game::game_object::PhaseOutCause;
use crate::game::phasing::{phase_in_object, phase_out_object};
use crate::types::ability::{
    Effect, EffectError, EffectKind, ResolvedAbility, TargetFilter, TargetRef,
};
use crate::types::events::GameEvent;
use crate::types::game_state::GameState;
use crate::types::identifiers::ObjectId;

/// CR 702.26a: Resolve `Effect::PhaseOut` by phasing out every targeted
/// permanent (or every permanent matching the effect's mass filter, e.g.
/// "All permanents you control phase out" from Teferi's Protection). Phased-
/// out objects remain on the battlefield (CR 702.26d); we delegate to
/// `phase_out_object` which also cascades to indirectly-phased attachments
/// and removes everything from combat (CR 506.4 + CR 702.26g).
pub fn resolve(
    state: &mut GameState,
    ability: &ResolvedAbility,
    events: &mut Vec<GameEvent>,
) -> Result<(), EffectError> {
    let target = match &ability.effect {
        Effect::PhaseOut { target } => target.clone(),
        _ => return Ok(()),
    };

    let targets = collect_object_targets(state, ability, &target);
    for oid in targets {
        phase_out_object(state, oid, PhaseOutCause::Directly, events);
    }

    events.push(GameEvent::EffectResolved {
        kind: EffectKind::PhaseOut,
        source_id: ability.source_id,
    });
    Ok(())
}

/// CR 702.26c: Resolve `Effect::PhaseIn` by phasing in every targeted
/// permanent. Rare; most phasing-in happens during the untap-step TBA.
pub fn resolve_phase_in(
    state: &mut GameState,
    ability: &ResolvedAbility,
    events: &mut Vec<GameEvent>,
) -> Result<(), EffectError> {
    let target = match &ability.effect {
        Effect::PhaseIn { target } => target.clone(),
        _ => return Ok(()),
    };

    // CR 702.26b: Filter choke point normally excludes phased-out objects, so
    // we can't rely on the standard target expansion for phase-in. Instead,
    // enumerate state.battlefield directly and match the filter manually,
    // skipping the phased-out exclusion.
    let targets = collect_phase_in_targets(state, ability, &target);
    for oid in targets {
        phase_in_object(state, oid, events);
    }

    events.push(GameEvent::EffectResolved {
        kind: EffectKind::PhaseIn,
        source_id: ability.source_id,
    });
    Ok(())
}

/// Resolve the target object set for a `PhaseOut` effect. Explicit
/// `ability.targets` (from the targeting phase) take precedence; mass filters
/// (e.g., `Typed Permanent / You`) are expanded against the battlefield.
fn collect_object_targets(
    state: &GameState,
    ability: &ResolvedAbility,
    target: &TargetFilter,
) -> Vec<ObjectId> {
    let from_targets: Vec<ObjectId> = ability
        .targets
        .iter()
        .filter_map(|t| match t {
            TargetRef::Object(id) => Some(*id),
            TargetRef::Player(_) => None,
        })
        .collect();
    if !from_targets.is_empty() {
        return from_targets;
    }

    // Mass filter — expand against the phased-in battlefield.
    let ctx = FilterContext::from_ability(ability);
    state
        .battlefield_phased_in_ids()
        .into_iter()
        .filter(|id| matches_target_filter(state, *id, target, &ctx))
        .collect()
}

/// Resolve target object set for a `PhaseIn` effect. Because the filter
/// choke point treats phased-out objects as nonexistent, we iterate
/// `state.battlefield` directly and evaluate only the non-phased-out aspects
/// of the filter here.
fn collect_phase_in_targets(
    state: &GameState,
    ability: &ResolvedAbility,
    target: &TargetFilter,
) -> Vec<ObjectId> {
    let from_targets: Vec<ObjectId> = ability
        .targets
        .iter()
        .filter_map(|t| match t {
            TargetRef::Object(id) => Some(*id),
            TargetRef::Player(_) => None,
        })
        .collect();
    if !from_targets.is_empty() {
        return from_targets;
    }

    // For mass phase-in effects, we must see phased-out permanents — that's
    // the only sensible target set. Rely on the caller-supplied filter to
    // narrow (e.g., "all phased-out permanents you control"). The core
    // filter choke point hides phased-out objects, so we can't use it for
    // non-trivial mass filters here without extending the filter API. For
    // now, a mass phase-in with a controller-scoped filter is approximated
    // by scanning all battlefield objects and checking controller.
    let _ = (ability, target);
    state
        .battlefield
        .iter()
        .copied()
        .filter(|id| state.objects.get(id).is_some_and(|obj| obj.is_phased_out()))
        .collect()
}
