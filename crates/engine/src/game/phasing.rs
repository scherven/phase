//! CR 702.26: Phasing — the status-based "treat as though it does not exist"
//! mechanic. Phased-out permanents remain in `Zone::Battlefield` (CR 702.26d:
//! phasing never causes a zone change); their `GameObject::phase_status`
//! discriminates the two states.
//!
//! Architectural invariants:
//! - Filter exclusion lives in `game/filter.rs::filter_inner` and
//!   `game/targeting.rs::zone_object_ids` (the single choke points). All other
//!   callers get phased-out exclusion transparently.
//! - Zone doesn't change — never emit `ZoneChanged` (CR 702.26d).
//! - Counters, stickers, attachments, is_commander, chosen_attributes all
//!   persist unchanged across a phase-out/phase-in cycle.
//! - Combat involvement clears on phase-out (CR 702.26b + CR 506.4).

use std::collections::HashSet;

use crate::game::effects::remove_from_combat::remove_object_from_combat;
use crate::game::game_object::{PhaseOutCause, PhaseStatus};
use crate::types::card_type::CoreType;
use crate::types::events::GameEvent;
use crate::types::game_state::GameState;
use crate::types::identifiers::ObjectId;

/// CR 702.26b: Phase out a permanent directly, cascading indirect phase-out
/// through Auras/Equipment/Fortifications attached to it (CR 702.26g).
///
/// Returns the set of object ids that transitioned to phased-out during this
/// call (empty if the target was already phased out or didn't exist). Useful
/// for `Effect::PhaseOut` resolvers that emit individual events and for
/// aggregate callers (Teferi's Protection, Teferi's Realm) that phase many
/// permanents at once.
pub fn phase_out_object(
    state: &mut GameState,
    object_id: ObjectId,
    cause: PhaseOutCause,
    events: &mut Vec<GameEvent>,
) -> Vec<ObjectId> {
    let mut phased: Vec<ObjectId> = Vec::new();
    let mut queue: Vec<(ObjectId, PhaseOutCause)> = vec![(object_id, cause)];
    let mut seen: HashSet<ObjectId> = HashSet::new();

    while let Some((id, this_cause)) = queue.pop() {
        if !seen.insert(id) {
            continue;
        }
        let Some(obj) = state.objects.get_mut(&id) else {
            continue;
        };
        // Already phased out: CR 702.26h — direct-over-indirect preference
        // handled by not downgrading an existing direct phase-out to indirect.
        if obj.is_phased_out() {
            if matches!(this_cause, PhaseOutCause::Directly) {
                // Upgrade indirect → direct per CR 702.26h.
                obj.phase_status = PhaseStatus::PhasedOut {
                    cause: PhaseOutCause::Directly,
                };
            }
            continue;
        }

        obj.phase_status = PhaseStatus::PhasedOut { cause: this_cause };
        phased.push(id);

        // CR 702.26g: cascade to attached Auras/Equipment/Fortifications.
        // One level deep only — the spec wording "any Auras, Equipment, or
        // Fortifications attached to that permanent" refers to direct
        // attachments; attachments-of-attachments remain in their attached
        // state (the host they were attached to didn't phase out — we did).
        let attachments = state
            .objects
            .get(&id)
            .map(|o| o.attachments.clone())
            .unwrap_or_default();
        for att_id in attachments {
            if let Some(att) = state.objects.get(&att_id) {
                if is_attachment_cascaded_by_phasing(&att.card_types.core_types) {
                    queue.push((att_id, PhaseOutCause::Indirectly));
                }
            }
        }
    }

    // CR 506.4 + CR 702.26b: Removal from combat happens once all cascades
    // settle, so concurrent attacker/blocker updates apply to the full set.
    for &id in &phased {
        remove_object_from_combat(state, id);
    }

    // Emit events after all mutation, so observers see a consistent state.
    for &id in &phased {
        let indirect = matches!(
            state
                .objects
                .get(&id)
                .map(|o| o.phase_status)
                .unwrap_or_default(),
            PhaseStatus::PhasedOut {
                cause: PhaseOutCause::Indirectly
            }
        );
        events.push(GameEvent::PermanentPhasedOut {
            object_id: id,
            indirect,
        });
    }

    phased
}

/// CR 702.26c / CR 702.26g: Phase in a permanent. Directly-phased-out
/// permanents phase in on their own; indirectly-phased-out ones phase in
/// alongside the host they were attached to (follow the attachment chain).
///
/// Returns the set of object ids that transitioned to phased-in.
pub fn phase_in_object(
    state: &mut GameState,
    object_id: ObjectId,
    events: &mut Vec<GameEvent>,
) -> Vec<ObjectId> {
    let mut phased: Vec<ObjectId> = Vec::new();
    let mut queue: Vec<ObjectId> = vec![object_id];
    let mut seen: HashSet<ObjectId> = HashSet::new();

    while let Some(id) = queue.pop() {
        if !seen.insert(id) {
            continue;
        }
        let Some(obj) = state.objects.get_mut(&id) else {
            continue;
        };
        if obj.is_phased_in() {
            continue;
        }
        obj.phase_status = PhaseStatus::PhasedIn;
        phased.push(id);

        // CR 702.26g: phase in any permanents that phased out indirectly
        // because this one phased out (they were attached to it). They ride
        // along with the host.
        let attachments = state
            .objects
            .get(&id)
            .map(|o| o.attachments.clone())
            .unwrap_or_default();
        for att_id in attachments {
            if let Some(att) = state.objects.get(&att_id) {
                if matches!(
                    att.phase_status,
                    PhaseStatus::PhasedOut {
                        cause: PhaseOutCause::Indirectly
                    }
                ) {
                    queue.push(att_id);
                }
            }
        }
    }

    for &id in &phased {
        events.push(GameEvent::PermanentPhasedIn { object_id: id });
    }

    phased
}

/// CR 702.26g: Only Auras, Equipment, and Fortifications cascade when the
/// host phases out.
fn is_attachment_cascaded_by_phasing(core_types: &[CoreType]) -> bool {
    core_types.iter().any(|t| {
        matches!(
            t,
            CoreType::Enchantment | CoreType::Artifact // Auras are Enchantments; Equipment and Fortifications are Artifacts.
        )
    })
}

/// CR 502.1 + CR 702.26a: Perform the untap-step phasing turn-based action
/// for the active player. All phased-in permanents the player controls that
/// have phasing phase out; simultaneously all phased-out permanents that had
/// phased out under that player's control phase in.
///
/// CR 702.26m: If the untap step itself is skipped, phasing is also skipped.
/// This TBA must be called only when the untap step is actually happening.
pub fn execute_untap_step_phasing(state: &mut GameState, events: &mut Vec<GameEvent>) {
    use crate::types::keywords::Keyword;
    let active = state.active_player;

    // Collect BEFORE mutating: snapshot all target ids so simultaneous
    // phase-in + phase-out semantics hold (CR 702.26a "simultaneously").
    //
    // CR 702.26a: "all phased-in permanents with phasing that player controls
    // phase out" — look at the controller's on-battlefield phased-in objects
    // with the Phasing keyword.
    //
    // CR 702.26a: "all phased-out permanents that had phased out under that
    // player's control phase in". We identify these by scanning all objects
    // whose last-known controller is `active` and whose phase_status is
    // `PhasedOut` directly (indirect ones ride along with their host).
    let phase_out_ids: Vec<ObjectId> = state
        .battlefield
        .iter()
        .copied()
        .filter(|id| {
            state.objects.get(id).is_some_and(|obj| {
                obj.is_phased_in() && obj.controller == active && obj.has_keyword(&Keyword::Phasing)
            })
        })
        .collect();

    let phase_in_ids: Vec<ObjectId> = state
        .battlefield
        .iter()
        .copied()
        .filter(|id| {
            state.objects.get(id).is_some_and(|obj| {
                matches!(
                    obj.phase_status,
                    PhaseStatus::PhasedOut {
                        cause: PhaseOutCause::Directly
                    }
                ) && obj.controller == active
            })
        })
        .collect();

    // CR 702.26a: simultaneity. We perform both in one pass so that e.g. a
    // creature with phasing doesn't immediately phase back in, and a
    // phased-out permanent doesn't phase out again this same step.
    for id in phase_in_ids {
        phase_in_object(state, id, events);
    }
    for id in phase_out_ids {
        phase_out_object(state, id, PhaseOutCause::Directly, events);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::zones::create_object;
    use crate::types::identifiers::CardId;
    use crate::types::keywords::Keyword;
    use crate::types::player::PlayerId;
    use crate::types::zones::Zone;

    fn setup_creature(state: &mut GameState, name: &str, controller: PlayerId) -> ObjectId {
        let id = create_object(
            state,
            CardId(1),
            controller,
            name.to_string(),
            Zone::Battlefield,
        );
        if let Some(obj) = state.objects.get_mut(&id) {
            obj.card_types.core_types = vec![CoreType::Creature];
            obj.power = Some(2);
            obj.toughness = Some(2);
            obj.base_power = Some(2);
            obj.base_toughness = Some(2);
        }
        id
    }

    fn setup_aura(
        state: &mut GameState,
        name: &str,
        controller: PlayerId,
        attached_to: ObjectId,
    ) -> ObjectId {
        let id = create_object(
            state,
            CardId(2),
            controller,
            name.to_string(),
            Zone::Battlefield,
        );
        if let Some(obj) = state.objects.get_mut(&id) {
            obj.card_types.core_types = vec![CoreType::Enchantment];
            obj.card_types.subtypes = vec!["Aura".to_string()];
            obj.attached_to = Some(attached_to);
        }
        if let Some(host) = state.objects.get_mut(&attached_to) {
            host.attachments.push(id);
        }
        id
    }

    #[test]
    fn phase_out_transitions_status_and_emits_event() {
        let mut state = GameState::new_two_player(42);
        let id = setup_creature(&mut state, "Breezekeeper", PlayerId(0));
        let mut events = Vec::new();

        let phased = phase_out_object(&mut state, id, PhaseOutCause::Directly, &mut events);

        assert_eq!(phased, vec![id]);
        assert!(state.objects[&id].is_phased_out());
        assert!(events.iter().any(|e| matches!(
            e,
            GameEvent::PermanentPhasedOut {
                object_id,
                indirect: false,
            } if *object_id == id
        )));
    }

    #[test]
    fn phase_out_cascades_to_attached_aura() {
        let mut state = GameState::new_two_player(42);
        let creature = setup_creature(&mut state, "Bear", PlayerId(0));
        let aura = setup_aura(&mut state, "Boon", PlayerId(0), creature);
        let mut events = Vec::new();

        phase_out_object(&mut state, creature, PhaseOutCause::Directly, &mut events);

        assert!(state.objects[&creature].is_phased_out());
        assert!(state.objects[&aura].is_phased_out());
        assert!(matches!(
            state.objects[&aura].phase_status,
            PhaseStatus::PhasedOut {
                cause: PhaseOutCause::Indirectly
            }
        ));
    }

    #[test]
    fn phase_in_cascades_to_indirectly_phased_attachments() {
        let mut state = GameState::new_two_player(42);
        let creature = setup_creature(&mut state, "Bear", PlayerId(0));
        let aura = setup_aura(&mut state, "Boon", PlayerId(0), creature);
        let mut events = Vec::new();

        phase_out_object(&mut state, creature, PhaseOutCause::Directly, &mut events);
        events.clear();

        phase_in_object(&mut state, creature, &mut events);

        assert!(state.objects[&creature].is_phased_in());
        assert!(state.objects[&aura].is_phased_in());
    }

    #[test]
    fn untap_step_phasing_toggles_phasing_keyword_permanents() {
        let mut state = GameState::new_two_player(42);
        state.active_player = PlayerId(0);
        let id = setup_creature(&mut state, "Breezekeeper", PlayerId(0));
        state.objects.get_mut(&id).unwrap().keywords = vec![Keyword::Phasing];
        state.objects.get_mut(&id).unwrap().base_keywords = vec![Keyword::Phasing];
        let mut events = Vec::new();

        // Turn 1: phase out on the active player's untap step.
        execute_untap_step_phasing(&mut state, &mut events);
        assert!(state.objects[&id].is_phased_out());

        events.clear();

        // Turn 2: phase in on the same player's next untap step.
        execute_untap_step_phasing(&mut state, &mut events);
        assert!(state.objects[&id].is_phased_in());
    }

    /// CR 702.26b: A phased-out creature can't be targeted — the filter
    /// choke point in `filter_inner` excludes phased-out objects.
    #[test]
    fn phased_out_creature_is_not_targetable() {
        use crate::game::targeting::find_legal_targets;
        use crate::types::ability::{TargetFilter, TargetRef, TypeFilter, TypedFilter};

        let mut state = GameState::new_two_player(42);
        let id = setup_creature(&mut state, "Bear", PlayerId(0));
        let source = setup_creature(&mut state, "Source", PlayerId(1));
        let mut events = Vec::new();

        // Before phase-out: targetable.
        let filter = TargetFilter::Typed(TypedFilter {
            type_filters: vec![TypeFilter::Creature],
            ..Default::default()
        });
        let legal_before = find_legal_targets(&state, &filter, PlayerId(1), source);
        assert!(legal_before.contains(&TargetRef::Object(id)));

        phase_out_object(&mut state, id, PhaseOutCause::Directly, &mut events);

        // After phase-out: not targetable.
        let legal_after = find_legal_targets(&state, &filter, PlayerId(1), source);
        assert!(!legal_after.contains(&TargetRef::Object(id)));
    }

    /// CR 702.26b + CR 506.4: A phased-out creature is removed from combat.
    #[test]
    fn phase_out_removes_from_combat() {
        use crate::game::combat::{AttackTarget, AttackerInfo, CombatState};

        let mut state = GameState::new_two_player(42);
        let id = setup_creature(&mut state, "Attacker", PlayerId(0));
        state.combat = Some(CombatState {
            attackers: vec![AttackerInfo {
                object_id: id,
                defending_player: PlayerId(1),
                attack_target: AttackTarget::Player(PlayerId(1)),
                blocked: false,
            }],
            ..Default::default()
        });
        let mut events = Vec::new();

        phase_out_object(&mut state, id, PhaseOutCause::Directly, &mut events);

        let combat = state.combat.as_ref().unwrap();
        assert!(
            combat.attackers.is_empty(),
            "Phased-out creature must leave combat (CR 506.4 + CR 702.26b)"
        );
    }

    /// CR 702.26d: Counters persist across a phase-out/phase-in cycle.
    #[test]
    fn counters_persist_through_phasing() {
        use crate::types::counter::CounterType;

        let mut state = GameState::new_two_player(42);
        let id = setup_creature(&mut state, "Bear", PlayerId(0));
        state
            .objects
            .get_mut(&id)
            .unwrap()
            .counters
            .insert(CounterType::Plus1Plus1, 3);
        let mut events = Vec::new();

        phase_out_object(&mut state, id, PhaseOutCause::Directly, &mut events);
        assert_eq!(
            state.objects[&id]
                .counters
                .get(&CounterType::Plus1Plus1)
                .copied(),
            Some(3),
            "Counters must not be removed by phase-out (CR 702.26d)"
        );

        phase_in_object(&mut state, id, &mut events);
        assert_eq!(
            state.objects[&id]
                .counters
                .get(&CounterType::Plus1Plus1)
                .copied(),
            Some(3),
            "Counters must persist across the phase-in (CR 702.26d)"
        );
    }

    /// CR 702.26d: "Tokens continue to exist on the battlefield while phased
    /// out" — a phased-out token stays on the battlefield.
    #[test]
    fn phased_out_token_persists() {
        let mut state = GameState::new_two_player(42);
        let id = setup_creature(&mut state, "Spirit Token", PlayerId(0));
        state.objects.get_mut(&id).unwrap().is_token = true;
        let mut events = Vec::new();

        phase_out_object(&mut state, id, PhaseOutCause::Directly, &mut events);
        assert_eq!(state.objects[&id].zone, Zone::Battlefield);
        assert!(state.objects[&id].is_phased_out());
        assert!(state.objects[&id].is_token);
    }

    /// CR 702.26e: Continuous effects from a phased-out source don't apply.
    /// Exercised via `collect_shared_active_continuous_effects` which skips
    /// phased-out battlefield objects.
    #[test]
    fn continuous_effects_skip_phased_out_source() {
        use crate::game::layers::collect_shared_active_continuous_effects;
        use crate::types::ability::{ContinuousModification, StaticDefinition, TargetFilter};
        use crate::types::statics::StaticMode;

        let mut state = GameState::new_two_player(42);
        let anthem_id = setup_creature(&mut state, "Glorious Anthem", PlayerId(0));
        // Attach an "other creatures get +1/+1" continuous static.
        let anthem_static = StaticDefinition::new(StaticMode::Continuous)
            .modifications(vec![ContinuousModification::AddPower { value: 1 }])
            .affected(TargetFilter::Any);
        state
            .objects
            .get_mut(&anthem_id)
            .unwrap()
            .static_definitions = vec![anthem_static.clone()];
        state
            .objects
            .get_mut(&anthem_id)
            .unwrap()
            .base_static_definitions = vec![anthem_static];

        let before = collect_shared_active_continuous_effects(&state);
        assert!(
            !before.is_empty(),
            "Phased-in anthem should contribute effects"
        );

        let mut events = Vec::new();
        phase_out_object(&mut state, anthem_id, PhaseOutCause::Directly, &mut events);

        let after = collect_shared_active_continuous_effects(&state);
        assert!(
            after.is_empty(),
            "Phased-out anthem must contribute no continuous effects (CR 702.26e)"
        );
    }

    /// CR 702.26g: When the active player's untap step arrives, an aura
    /// that phased out indirectly phases back in along with its host, not
    /// on its own. Test both phase-out *and* the host-driven phase-in.
    #[test]
    fn indirect_aura_phases_back_with_host_on_untap() {
        let mut state = GameState::new_two_player(42);
        state.active_player = PlayerId(0);
        let creature = setup_creature(&mut state, "Bear", PlayerId(0));
        state.objects.get_mut(&creature).unwrap().keywords = vec![Keyword::Phasing];
        state.objects.get_mut(&creature).unwrap().base_keywords = vec![Keyword::Phasing];
        let aura = setup_aura(&mut state, "Boon", PlayerId(0), creature);
        let mut events = Vec::new();

        // First untap step: creature phases out, aura cascades indirectly.
        execute_untap_step_phasing(&mut state, &mut events);
        assert!(state.objects[&creature].is_phased_out());
        assert!(matches!(
            state.objects[&aura].phase_status,
            PhaseStatus::PhasedOut {
                cause: PhaseOutCause::Indirectly
            }
        ));

        events.clear();

        // Second untap step: creature phases back in directly; aura rides
        // along (doesn't phase in on its own — it's indirect).
        execute_untap_step_phasing(&mut state, &mut events);
        assert!(state.objects[&creature].is_phased_in());
        assert!(
            state.objects[&aura].is_phased_in(),
            "Aura must phase in with its host (CR 702.26g)"
        );
    }
}
