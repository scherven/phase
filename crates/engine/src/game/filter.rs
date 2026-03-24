//! Typed object filter matching using TargetFilter enum.
//!
//! Replaces the Forge-style string filter parsing with typed enum matching.
//! All filter logic works against the TargetFilter enum hierarchy from types/ability.rs.

use std::collections::HashSet;

use crate::game::combat;
use crate::game::game_object::{parse_counter_type, GameObject};
use crate::game::quantity::resolve_quantity;
use crate::types::ability::{
    ControllerRef, FilterProp, TargetFilter, TargetRef, TypeFilter, TypedFilter,
};
use crate::types::card_type::CoreType;
use crate::types::game_state::GameState;
use crate::types::identifiers::ObjectId;
use crate::types::mana::ManaColor;
use crate::types::player::PlayerId;

/// Check if an object matches a typed TargetFilter.
///
/// `source_id` is the object that owns the ability (used for SelfRef/Other resolution).
/// The source's controller is looked up from `state` (for You/Opponent).
pub fn matches_target_filter(
    state: &GameState,
    object_id: ObjectId,
    filter: &TargetFilter,
    source_id: ObjectId,
) -> bool {
    let source_controller = state.objects.get(&source_id).map(|o| o.controller);
    filter_inner(state, object_id, filter, source_id, source_controller)
}

/// Like [`matches_target_filter`], but with an explicit controller.
///
/// Use this when resolving effects from the stack where the source object
/// may no longer exist (e.g. sacrificed as a cost).
pub fn matches_target_filter_controlled(
    state: &GameState,
    object_id: ObjectId,
    filter: &TargetFilter,
    source_id: ObjectId,
    controller: PlayerId,
) -> bool {
    filter_inner(state, object_id, filter, source_id, Some(controller))
}

fn filter_inner(
    state: &GameState,
    object_id: ObjectId,
    filter: &TargetFilter,
    source_id: ObjectId,
    source_controller: Option<PlayerId>,
) -> bool {
    match filter {
        TargetFilter::None => false,
        TargetFilter::Any => true,
        TargetFilter::Player => false,     // Players are not objects
        TargetFilter::Controller => false, // Controller is a player, not an object
        TargetFilter::SelfRef => object_id == source_id,
        TargetFilter::Typed(TypedFilter {
            type_filters,
            controller,
            properties,
        }) => {
            let obj = match state.objects.get(&object_id) {
                Some(o) => o,
                None => return false,
            };
            // Type filters check (all must match — conjunction)
            for tf in type_filters {
                if !type_filter_matches(tf, obj) {
                    return false;
                }
            }
            // Controller check
            if let Some(ctrl) = controller {
                match ctrl {
                    ControllerRef::You => {
                        if source_controller != Some(obj.controller) {
                            return false;
                        }
                    }
                    ControllerRef::Opponent => {
                        if source_controller == Some(obj.controller) {
                            return false;
                        }
                    }
                }
            }
            // All properties must match
            let source_obj = state.objects.get(&source_id);
            let source_attached_to = source_obj.and_then(|s| s.attached_to);
            let source_chosen_creature_type =
                source_obj.and_then(|s| s.chosen_creature_type().map(|t| t.to_string()));
            let source_ctx = SourceContext {
                id: source_id,
                controller: source_controller,
                attached_to: source_attached_to,
                chosen_creature_type: source_chosen_creature_type.as_deref(),
            };
            properties
                .iter()
                .all(|p| matches_filter_prop(p, state, obj, object_id, &source_ctx))
        }
        TargetFilter::Not { filter: inner } => {
            !filter_inner(state, object_id, inner, source_id, source_controller)
        }
        TargetFilter::Or { filters } => filters
            .iter()
            .any(|f| filter_inner(state, object_id, f, source_id, source_controller)),
        TargetFilter::And { filters } => filters
            .iter()
            .all(|f| filter_inner(state, object_id, f, source_id, source_controller)),
        // StackAbility/StackSpell targeting is handled directly at call sites, not via filter
        TargetFilter::StackAbility | TargetFilter::StackSpell => false,
        TargetFilter::SpecificObject { id: target_id } => object_id == *target_id,
        TargetFilter::AttachedTo => state
            .objects
            .get(&source_id)
            .and_then(|src| src.attached_to)
            .is_some_and(|attached| attached == object_id),
        TargetFilter::LastCreated => state.last_created_token_ids.contains(&object_id),
        // CR 603.7: Match objects in a tracked set from the originating effect.
        TargetFilter::TrackedSet { id } => state
            .tracked_object_sets
            .get(id)
            .is_some_and(|set| set.contains(&object_id)),
        // CR 607.2a + CR 406.6: Match cards exiled by source via exile-until-leaves links.
        // CR 610.3: Linked abilities track which cards were exiled by the first ability.
        TargetFilter::ExiledBySource => state
            .exile_links
            .iter()
            .any(|link| link.source_id == source_id && link.exiled_id == object_id),
        // CR 603.7c: Event-context references resolve to players, not objects.
        TargetFilter::TriggeringSpellController
        | TargetFilter::TriggeringSpellOwner
        | TargetFilter::TriggeringPlayer
        | TargetFilter::TriggeringSource
        | TargetFilter::DefendingPlayer => false,
        // ParentTarget resolves to parent ability's targets at resolution time,
        // not via object matching.
        TargetFilter::ParentTarget => false,
    }
}

/// Check if an object matches a TypeFilter variant.
/// Check if an object's card types match a `TypeFilter`.
/// CR 205.2a: Each card type has its own rules for how it behaves.
/// Public for use by trigger_matchers and other modules that need type checking.
pub fn type_filter_matches(tf: &TypeFilter, obj: &GameObject) -> bool {
    match tf {
        TypeFilter::Creature => obj.card_types.core_types.contains(&CoreType::Creature),
        TypeFilter::Land => obj.card_types.core_types.contains(&CoreType::Land),
        // CR 301: Artifact type check.
        TypeFilter::Artifact => obj.card_types.core_types.contains(&CoreType::Artifact),
        TypeFilter::Enchantment => obj.card_types.core_types.contains(&CoreType::Enchantment),
        // CR 304: Instant type check.
        TypeFilter::Instant => obj.card_types.core_types.contains(&CoreType::Instant),
        TypeFilter::Sorcery => obj.card_types.core_types.contains(&CoreType::Sorcery),
        // CR 306: Planeswalker type check.
        TypeFilter::Planeswalker => obj.card_types.core_types.contains(&CoreType::Planeswalker),
        // CR 403.3: Permanents exist only on the battlefield — creatures, artifacts, enchantments, lands, planeswalkers.
        TypeFilter::Permanent => {
            obj.card_types.core_types.contains(&CoreType::Creature)
                || obj.card_types.core_types.contains(&CoreType::Artifact)
                || obj.card_types.core_types.contains(&CoreType::Enchantment)
                || obj.card_types.core_types.contains(&CoreType::Land)
                || obj.card_types.core_types.contains(&CoreType::Planeswalker)
        }
        TypeFilter::Card | TypeFilter::Any => true,
        TypeFilter::Non(inner) => !type_filter_matches(inner, obj),
        // CR 205.3: Subtype matching — Changeling (CR 702.73) types are expanded
        // by the layer system before this check, so obj.card_types.subtypes is complete.
        TypeFilter::Subtype(ref sub) => obj
            .card_types
            .subtypes
            .iter()
            .any(|s| s.eq_ignore_ascii_case(sub)),
        // CR 608.2b: Disjunction — matches if any inner filter matches.
        TypeFilter::AnyOf(ref filters) => filters.iter().any(|f| type_filter_matches(f, obj)),
    }
}

/// Context about the source of an ability, used during filter property evaluation.
struct SourceContext<'a> {
    id: ObjectId,
    controller: Option<PlayerId>,
    attached_to: Option<ObjectId>,
    chosen_creature_type: Option<&'a str>,
}

/// Check if an object satisfies a single FilterProp.
fn matches_filter_prop(
    prop: &FilterProp,
    state: &GameState,
    obj: &GameObject,
    object_id: ObjectId,
    source: &SourceContext<'_>,
) -> bool {
    match prop {
        FilterProp::Token => {
            // A token has no card_id (card_id.0 == 0) in typical token creation
            // For now, permissive true -- tokens will be marked more explicitly later
            true
        }
        FilterProp::Attacking => state.combat.as_ref().is_some_and(|combat| {
            combat
                .attackers
                .iter()
                .any(|attacker| attacker.object_id == object_id)
        }),
        // CR 509.1h: Unblocked = attacking creature that was never assigned blockers.
        // unblocked_attackers checks the permanent `blocked` flag, not the current blocker list.
        FilterProp::Unblocked => combat::unblocked_attackers(state).contains(&object_id),
        FilterProp::Tapped => obj.tapped,
        // CR 302.6 / CR 110.5: Untapped status as targeting qualifier.
        FilterProp::Untapped => !obj.tapped,
        FilterProp::WithKeyword { value } => {
            // Check if object has the keyword
            let kw: Result<crate::types::keywords::Keyword, _> = value.parse();
            match kw {
                Ok(k) => obj.has_keyword(&k),
                Err(_) => true, // Unknown keyword -- permissive
            }
        }
        FilterProp::CountersGE {
            counter_type,
            count,
        } => {
            let ct = parse_counter_type(counter_type);
            obj.counters.get(&ct).copied().unwrap_or(0) >= *count
        }
        FilterProp::CmcGE { value } => {
            let cmc = obj.mana_cost.mana_value() as i32;
            let controller = source.controller.unwrap_or(PlayerId(0));
            let threshold = resolve_quantity(state, value, controller, source.id);
            cmc >= threshold
        }
        FilterProp::CmcLE { value } => {
            let cmc = obj.mana_cost.mana_value() as i32;
            let controller = source.controller.unwrap_or(PlayerId(0));
            let threshold = resolve_quantity(state, value, controller, source.id);
            cmc <= threshold
        }
        // CR 202.3: Exact mana value match.
        FilterProp::CmcEQ { value } => {
            let cmc = obj.mana_cost.mana_value() as i32;
            let controller = source.controller.unwrap_or(PlayerId(0));
            let threshold = resolve_quantity(state, value, controller, source.id);
            cmc == threshold
        }
        // SameName: matches objects with the same name as the tracked card from context.
        // At runtime, this checks against the source object's name (the event context card).
        FilterProp::SameName => {
            if let Some(source_obj) = state.objects.get(&source.id) {
                obj.name == source_obj.name
            } else {
                false
            }
        }
        FilterProp::InZone { zone } => obj.zone == *zone,
        FilterProp::Owned { controller } => match controller {
            ControllerRef::You => source.controller == Some(obj.owner),
            ControllerRef::Opponent => {
                source.controller.is_some() && source.controller != Some(obj.owner)
            }
        },
        FilterProp::EnchantedBy => source.attached_to == Some(object_id),
        FilterProp::EquippedBy => source.attached_to == Some(object_id),
        FilterProp::Another => object_id != source.id,
        FilterProp::HasColor { color } => {
            let mana_color = match color.as_str() {
                "White" => Some(ManaColor::White),
                "Blue" => Some(ManaColor::Blue),
                "Black" => Some(ManaColor::Black),
                "Red" => Some(ManaColor::Red),
                "Green" => Some(ManaColor::Green),
                _ => None,
            };
            match mana_color {
                Some(mc) => obj.color.contains(&mc),
                None => true, // Unknown color — permissive
            }
        }
        FilterProp::PowerLE { value } => obj.power.unwrap_or(0) <= *value,
        FilterProp::PowerGE { value } => obj.power.unwrap_or(0) >= *value,
        FilterProp::Multicolored => obj.color.len() > 1,
        FilterProp::HasSupertype { value } => {
            let st: Result<crate::types::card_type::Supertype, _> = value.parse();
            match st {
                Ok(supertype) => obj.card_types.supertypes.contains(&supertype),
                Err(_) => true, // Unknown supertype — permissive
            }
        }
        // CR 205.4b: Object does NOT have this color.
        FilterProp::NotColor { color } => {
            let mana_color = match color.as_str() {
                "White" => Some(ManaColor::White),
                "Blue" => Some(ManaColor::Blue),
                "Black" => Some(ManaColor::Black),
                "Red" => Some(ManaColor::Red),
                "Green" => Some(ManaColor::Green),
                _ => None,
            };
            match mana_color {
                Some(mc) => !obj.color.contains(&mc),
                None => true, // Unknown color — permissive
            }
        }
        // CR 205.4a: Object does NOT have this supertype.
        FilterProp::NotSupertype { value } => {
            let st: Result<crate::types::card_type::Supertype, _> = value.parse();
            match st {
                Ok(supertype) => !obj.card_types.supertypes.contains(&supertype),
                Err(_) => true, // Unknown supertype — permissive
            }
        }
        FilterProp::IsChosenCreatureType => match source.chosen_creature_type {
            Some(chosen) => obj
                .card_types
                .subtypes
                .iter()
                .any(|s| s.eq_ignore_ascii_case(chosen)),
            None => false,
        },
        // CR 702.157a: Match creatures with the suspected designation.
        FilterProp::Suspected => obj.is_suspected,
        // CR 510.1c: Match creatures whose toughness exceeds their power.
        FilterProp::ToughnessGTPower => {
            let power = obj.power.unwrap_or(0);
            let toughness = obj.toughness.unwrap_or(0);
            toughness > power
        }
        // Match objects whose name differs from all controlled battlefield objects matching the filter.
        FilterProp::DifferentNameFrom { filter } => {
            let controller = source.controller.unwrap_or(PlayerId(0));
            let controlled_names: Vec<&str> = state
                .battlefield
                .iter()
                .filter_map(|&bid| state.objects.get(&bid))
                .filter(|bobj| bobj.controller == controller)
                .filter(|bobj| {
                    matches_target_filter_controlled(state, bobj.id, filter, source.id, controller)
                })
                .map(|bobj| bobj.name.as_str())
                .collect();
            !controlled_names.contains(&obj.name.as_str())
        }
        // CR 604.3: Match objects in any of the listed zones (OR semantics).
        FilterProp::InAnyZone { zones } => zones.contains(&obj.zone),
        // CR 601.2c: Group constraint — not evaluable per-object, validated at resolution time.
        FilterProp::SharesQuality { .. } => true,
        // CR 510.1: Object was dealt damage this turn (damage_marked persists until cleanup).
        FilterProp::WasDealtDamageThisTurn => obj.damage_marked > 0,
        // CR 400.7: Object entered the battlefield this turn.
        FilterProp::EnteredThisTurn => obj.entered_battlefield_turn == Some(state.turn_number),
        // CR 508.1a: Creature was declared as an attacker this turn.
        FilterProp::AttackedThisTurn => state.creatures_attacked_this_turn.contains(&object_id),
        // CR 509.1a: Creature was declared as a blocker this turn.
        FilterProp::BlockedThisTurn => state.creatures_blocked_this_turn.contains(&object_id),
        // CR 508.1a + CR 509.1a: Creature attacked or blocked this turn.
        FilterProp::AttackedOrBlockedThisTurn => {
            state.creatures_attacked_this_turn.contains(&object_id)
                || state.creatures_blocked_this_turn.contains(&object_id)
        }
        // CR 115.7: Stack entry has exactly one target — permissive at filter level,
        // validated by retarget effects at resolution time.
        FilterProp::HasSingleTarget => true,
        // CR 115.9c: Stack entry's targets all match the inner filter — permissive at
        // per-object level, validated by trigger matchers and retarget effects against the
        // stack entry's actual targets.
        // CR 707.2: Match face-down permanents on the battlefield.
        FilterProp::FaceDown => obj.face_down,
        FilterProp::TargetsOnly { .. } => true,
        FilterProp::Other { .. } => true, // Permissive fallback for unrecognized properties
    }
}

/// CR 608.2b: Validate that all targeted objects share at least one value of the named quality.
/// This is a group constraint that cannot be checked per-object — it requires the full set.
/// Checked at resolution time per CR 608.2b (verifying target legality on resolution).
///
/// Returns `true` if the constraint is satisfied (or if there are fewer than 2 targets).
/// For "creature type": all objects must share at least one creature subtype.
/// For "color": all objects must share at least one color.
/// For "card type": all objects must share at least one card type.
pub fn validate_shares_quality(state: &GameState, targets: &[TargetRef], quality: &str) -> bool {
    let obj_ids: Vec<ObjectId> = targets
        .iter()
        .filter_map(|t| match t {
            TargetRef::Object(id) => Some(*id),
            TargetRef::Player(_) => None,
        })
        .collect();

    // Fewer than 2 objects — constraint is trivially satisfied.
    if obj_ids.len() < 2 {
        return true;
    }

    match quality {
        "creature type" | "creature types" => {
            // Collect subtypes for each object, then intersect.
            let mut subtype_sets: Vec<HashSet<&str>> = Vec::new();
            for &id in &obj_ids {
                if let Some(obj) = state.objects.get(&id) {
                    let set: HashSet<&str> =
                        obj.card_types.subtypes.iter().map(|s| s.as_str()).collect();
                    subtype_sets.push(set);
                } else {
                    return false;
                }
            }
            // Intersect all sets — at least one common subtype must exist.
            let mut shared = subtype_sets[0].clone();
            for set in &subtype_sets[1..] {
                shared = shared.intersection(set).copied().collect();
            }
            !shared.is_empty()
        }
        "color" => {
            // All objects must share at least one color.
            let mut color_sets: Vec<HashSet<&ManaColor>> = Vec::new();
            for &id in &obj_ids {
                if let Some(obj) = state.objects.get(&id) {
                    let set: HashSet<&ManaColor> = obj.color.iter().collect();
                    color_sets.push(set);
                } else {
                    return false;
                }
            }
            let mut shared = color_sets[0].clone();
            for set in &color_sets[1..] {
                shared = shared.intersection(set).copied().collect();
            }
            !shared.is_empty()
        }
        "card type" => {
            // All objects must share at least one core card type.
            let mut type_sets: Vec<HashSet<&CoreType>> = Vec::new();
            for &id in &obj_ids {
                if let Some(obj) = state.objects.get(&id) {
                    let set: HashSet<&CoreType> = obj.card_types.core_types.iter().collect();
                    type_sets.push(set);
                } else {
                    return false;
                }
            }
            let mut shared = type_sets[0].clone();
            for set in &type_sets[1..] {
                shared = shared.intersection(set).copied().collect();
            }
            !shared.is_empty()
        }
        // Unknown quality — permissive fallback.
        _ => true,
    }
}

/// Check if a player matches a typed player filter.
///
/// Used by static abilities that target players rather than objects.
pub fn player_matches_filter(
    player_id: PlayerId,
    filter: &str,
    source_controller: Option<PlayerId>,
) -> bool {
    for part in filter.split('+') {
        match part {
            "You" if source_controller != Some(player_id) => {
                return false;
            }
            "Opp" if source_controller == Some(player_id) => {
                return false;
            }
            _ => {}
        }
    }
    true
}

// ---------------------------------------------------------------------------
// CR 115.9c: "that targets only [X]" shared helpers
// ---------------------------------------------------------------------------

/// CR 115.9c: Extract the first `TargetsOnly` inner filter from a filter tree.
/// Walks through Or/And/Typed branches to find a `FilterProp::TargetsOnly`.
pub(crate) fn extract_targets_only(filter: &TargetFilter) -> Option<TargetFilter> {
    match filter {
        TargetFilter::Typed(tf) => {
            for prop in &tf.properties {
                if let FilterProp::TargetsOnly { filter } = prop {
                    return Some(*filter.clone());
                }
            }
            None
        }
        TargetFilter::Or { filters } | TargetFilter::And { filters } => {
            // All branches should have the same TargetsOnly (distributed by parser);
            // return the first one found.
            filters.iter().find_map(extract_targets_only)
        }
        _ => None,
    }
}

/// Check if a player target matches a TargetFilter constraint.
/// CR 115.9c: Used to validate player targets in "that targets only [X]" checks.
pub(crate) fn player_matches_target_filter(
    filter: &TargetFilter,
    player_id: PlayerId,
    source_controller: Option<PlayerId>,
) -> bool {
    match filter {
        TargetFilter::Any | TargetFilter::Player => true,
        TargetFilter::SelfRef => false, // SelfRef refers to objects, not players
        TargetFilter::Controller => source_controller == Some(player_id),
        TargetFilter::Typed(ref tf) if tf.type_filters.is_empty() => match &tf.controller {
            Some(ControllerRef::You) => source_controller == Some(player_id),
            Some(ControllerRef::Opponent) => source_controller.is_some_and(|c| c != player_id),
            None => true,
        },
        // Typed filters with type_filters don't match players
        TargetFilter::Typed(_) => false,
        TargetFilter::Or { filters } => filters
            .iter()
            .any(|f| player_matches_target_filter(f, player_id, source_controller)),
        TargetFilter::And { filters } => filters
            .iter()
            .all(|f| player_matches_target_filter(f, player_id, source_controller)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::zones::create_object;
    use crate::types::ability::{ControllerRef, FilterProp, TargetFilter};
    use crate::types::card_type::CoreType;
    use crate::types::identifiers::{CardId, ObjectId};
    use crate::types::player::PlayerId;
    use crate::types::zones::Zone;

    fn setup() -> GameState {
        GameState::new_two_player(42)
    }

    fn add_creature(state: &mut GameState, owner: PlayerId, name: &str) -> ObjectId {
        let id = create_object(
            state,
            CardId(state.next_object_id),
            owner,
            name.to_string(),
            crate::types::zones::Zone::Battlefield,
        );
        state
            .objects
            .get_mut(&id)
            .unwrap()
            .card_types
            .core_types
            .push(CoreType::Creature);
        id
    }

    #[test]
    fn none_filter_matches_nothing() {
        let mut state = setup();
        let id = add_creature(&mut state, PlayerId(0), "Bear");
        assert!(!matches_target_filter(&state, id, &TargetFilter::None, id));
    }

    #[test]
    fn any_filter_matches_everything() {
        let mut state = setup();
        let id = add_creature(&mut state, PlayerId(0), "Bear");
        assert!(matches_target_filter(&state, id, &TargetFilter::Any, id));
    }

    #[test]
    fn type_filter_matches_correct_type() {
        let mut state = setup();
        let id = add_creature(&mut state, PlayerId(0), "Bear");
        let creature_filter = TargetFilter::Typed(TypedFilter::creature());
        let land_filter = TargetFilter::Typed(TypedFilter::land());
        let card_filter = TargetFilter::Typed(TypedFilter::card());
        assert!(matches_target_filter(&state, id, &creature_filter, id));
        assert!(!matches_target_filter(&state, id, &land_filter, id));
        assert!(matches_target_filter(&state, id, &card_filter, id));
    }

    #[test]
    fn self_filter() {
        let mut state = setup();
        let a = add_creature(&mut state, PlayerId(0), "A");
        let b = add_creature(&mut state, PlayerId(0), "B");
        assert!(matches_target_filter(&state, a, &TargetFilter::SelfRef, a));
        assert!(!matches_target_filter(&state, b, &TargetFilter::SelfRef, a));
    }

    #[test]
    fn other_filter_excludes_source() {
        let mut state = setup();
        let marshal = add_creature(&mut state, PlayerId(0), "Benalish Marshal");
        let bear = add_creature(&mut state, PlayerId(0), "Bear");

        // "Creature.Other+YouCtrl" = And(Typed{creature, You}, Not(SelfRef))
        let filter = TargetFilter::And {
            filters: vec![
                TargetFilter::Typed(TypedFilter::creature().controller(ControllerRef::You)),
                TargetFilter::Not {
                    filter: Box::new(TargetFilter::SelfRef),
                },
            ],
        };

        // Marshal should NOT match its own "Other" filter
        assert!(!matches_target_filter(&state, marshal, &filter, marshal));
        // Bear should match
        assert!(matches_target_filter(&state, bear, &filter, marshal));
    }

    #[test]
    fn you_ctrl_filter() {
        let mut state = setup();
        let mine = add_creature(&mut state, PlayerId(0), "Mine");
        let theirs = add_creature(&mut state, PlayerId(1), "Theirs");

        let filter = TargetFilter::Typed(TypedFilter::creature().controller(ControllerRef::You));

        assert!(matches_target_filter(&state, mine, &filter, mine));
        assert!(!matches_target_filter(&state, theirs, &filter, mine));
    }

    #[test]
    fn opp_ctrl_filter() {
        let mut state = setup();
        let mine = add_creature(&mut state, PlayerId(0), "Mine");
        let theirs = add_creature(&mut state, PlayerId(1), "Theirs");

        let filter =
            TargetFilter::Typed(TypedFilter::creature().controller(ControllerRef::Opponent));

        assert!(!matches_target_filter(&state, mine, &filter, mine));
        assert!(matches_target_filter(&state, theirs, &filter, mine));
    }

    #[test]
    fn combined_type_and_controller() {
        let mut state = setup();
        let source = add_creature(&mut state, PlayerId(0), "Lord");
        let ally = add_creature(&mut state, PlayerId(0), "Ally");
        let enemy = add_creature(&mut state, PlayerId(1), "Enemy");

        // "Creature.Other+YouCtrl"
        let filter = TargetFilter::And {
            filters: vec![
                TargetFilter::Typed(TypedFilter::creature().controller(ControllerRef::You)),
                TargetFilter::Not {
                    filter: Box::new(TargetFilter::SelfRef),
                },
            ],
        };

        assert!(!matches_target_filter(&state, source, &filter, source));
        assert!(matches_target_filter(&state, ally, &filter, source));
        assert!(!matches_target_filter(&state, enemy, &filter, source));
    }

    #[test]
    fn permanent_matches_multiple_types() {
        let mut state = setup();
        let id = add_creature(&mut state, PlayerId(0), "Bear");
        let filter = TargetFilter::Typed(TypedFilter::permanent());
        assert!(matches_target_filter(&state, id, &filter, id));
    }

    #[test]
    fn enchanted_by_only_matches_attached_creature() {
        let mut state = setup();
        let creature_a = add_creature(&mut state, PlayerId(0), "Bear A");
        let creature_b = add_creature(&mut state, PlayerId(0), "Bear B");

        // Create an aura (source) attached to creature_a
        let next_id = state.next_object_id;
        let aura = create_object(
            &mut state,
            CardId(next_id),
            PlayerId(0),
            "Rancor".to_string(),
            crate::types::zones::Zone::Battlefield,
        );
        state
            .objects
            .get_mut(&aura)
            .unwrap()
            .card_types
            .core_types
            .push(CoreType::Enchantment);
        state.objects.get_mut(&aura).unwrap().attached_to = Some(creature_a);

        let filter =
            TargetFilter::Typed(TypedFilter::creature().properties(vec![FilterProp::EnchantedBy]));

        assert!(matches_target_filter(&state, creature_a, &filter, aura));
        assert!(
            !matches_target_filter(&state, creature_b, &filter, aura),
            "EnchantedBy must not match creatures the aura is NOT attached to"
        );
    }

    #[test]
    fn enchanted_by_no_attachment_matches_nothing() {
        let mut state = setup();
        let creature = add_creature(&mut state, PlayerId(0), "Bear");

        // Aura not attached to anything
        let next_id = state.next_object_id;
        let aura = create_object(
            &mut state,
            CardId(next_id),
            PlayerId(0),
            "Floating Aura".to_string(),
            crate::types::zones::Zone::Battlefield,
        );
        state
            .objects
            .get_mut(&aura)
            .unwrap()
            .card_types
            .core_types
            .push(CoreType::Enchantment);

        let filter =
            TargetFilter::Typed(TypedFilter::creature().properties(vec![FilterProp::EnchantedBy]));

        assert!(
            !matches_target_filter(&state, creature, &filter, aura),
            "Unattached aura should not match any creature"
        );
    }

    #[test]
    fn player_filter_you() {
        assert!(player_matches_filter(PlayerId(0), "You", Some(PlayerId(0))));
        assert!(!player_matches_filter(
            PlayerId(1),
            "You",
            Some(PlayerId(0))
        ));
    }

    #[test]
    fn player_filter_opp() {
        assert!(!player_matches_filter(
            PlayerId(0),
            "Opp",
            Some(PlayerId(0))
        ));
        assert!(player_matches_filter(PlayerId(1), "Opp", Some(PlayerId(0))));
    }

    #[test]
    fn not_filter_inverts() {
        let mut state = setup();
        let id = add_creature(&mut state, PlayerId(0), "Bear");
        let not_self = TargetFilter::Not {
            filter: Box::new(TargetFilter::SelfRef),
        };
        assert!(!matches_target_filter(&state, id, &not_self, id));
    }

    #[test]
    fn or_filter_any_match() {
        let mut state = setup();
        let id = add_creature(&mut state, PlayerId(0), "Bear");
        let filter = TargetFilter::Or {
            filters: vec![
                TargetFilter::Typed(TypedFilter::land()),
                TargetFilter::Typed(TypedFilter::creature()),
            ],
        };
        assert!(matches_target_filter(&state, id, &filter, id));
    }

    #[test]
    fn tapped_property() {
        let mut state = setup();
        let id = add_creature(&mut state, PlayerId(0), "Bear");
        state.objects.get_mut(&id).unwrap().tapped = true;

        let filter =
            TargetFilter::Typed(TypedFilter::default().properties(vec![FilterProp::Tapped]));
        assert!(matches_target_filter(&state, id, &filter, id));
    }

    #[test]
    fn has_supertype_basic_matches_basic_land() {
        let mut state = setup();
        let id = add_creature(&mut state, PlayerId(0), "Plains");
        state
            .objects
            .get_mut(&id)
            .unwrap()
            .card_types
            .supertypes
            .push(crate::types::card_type::Supertype::Basic);
        state.objects.get_mut(&id).unwrap().card_types.core_types = vec![CoreType::Land];

        let filter =
            TargetFilter::Typed(
                TypedFilter::land().properties(vec![FilterProp::HasSupertype {
                    value: "Basic".to_string(),
                }]),
            );
        assert!(matches_target_filter(&state, id, &filter, id));
    }

    #[test]
    fn has_supertype_basic_rejects_nonbasic_land() {
        let mut state = setup();
        let id = add_creature(&mut state, PlayerId(0), "Stomping Ground");
        state.objects.get_mut(&id).unwrap().card_types.core_types = vec![CoreType::Land];

        let filter =
            TargetFilter::Typed(
                TypedFilter::land().properties(vec![FilterProp::HasSupertype {
                    value: "Basic".to_string(),
                }]),
            );
        assert!(!matches_target_filter(&state, id, &filter, id));
    }

    #[test]
    fn controlled_variant_uses_explicit_controller() {
        let mut state = setup();
        let obj = add_creature(&mut state, PlayerId(1), "Theirs");

        let filter =
            TargetFilter::Typed(TypedFilter::creature().controller(ControllerRef::Opponent));

        // Source doesn't exist, but we pass controller explicitly
        let fake_source = ObjectId(9999);
        assert!(matches_target_filter_controlled(
            &state,
            obj,
            &filter,
            fake_source,
            PlayerId(0)
        ));
    }

    #[test]
    fn chosen_creature_type_matches_subtype() {
        use crate::types::ability::ChosenAttribute;

        let mut state = setup();
        let source = add_creature(&mut state, PlayerId(0), "Mimic");
        state
            .objects
            .get_mut(&source)
            .unwrap()
            .chosen_attributes
            .push(ChosenAttribute::CreatureType("Elf".to_string()));

        let elf = add_creature(&mut state, PlayerId(0), "Elf Warrior");
        state
            .objects
            .get_mut(&elf)
            .unwrap()
            .card_types
            .subtypes
            .push("Elf".to_string());

        let goblin = add_creature(&mut state, PlayerId(0), "Goblin");
        state
            .objects
            .get_mut(&goblin)
            .unwrap()
            .card_types
            .subtypes
            .push("Goblin".to_string());

        let filter = TargetFilter::Typed(
            TypedFilter::creature().properties(vec![FilterProp::IsChosenCreatureType]),
        );

        assert!(
            matches_target_filter(&state, elf, &filter, source),
            "Elf should match chosen creature type Elf"
        );
        assert!(
            !matches_target_filter(&state, goblin, &filter, source),
            "Goblin should not match chosen creature type Elf"
        );
    }

    #[test]
    fn attacking_property_matches_only_declared_attackers() {
        use crate::game::combat::{AttackerInfo, CombatState};

        let mut state = setup();
        let attacker = add_creature(&mut state, PlayerId(0), "Attacker");
        let bystander = add_creature(&mut state, PlayerId(0), "Bystander");
        state.combat = Some(CombatState {
            attackers: vec![AttackerInfo::new(attacker, PlayerId(1))],
            ..CombatState::default()
        });

        let filter =
            TargetFilter::Typed(TypedFilter::creature().properties(vec![FilterProp::Attacking]));

        assert!(matches_target_filter(&state, attacker, &filter, attacker));
        assert!(!matches_target_filter(&state, bystander, &filter, attacker));
    }

    #[test]
    fn exiled_by_source_matches_linked_objects() {
        use crate::types::game_state::ExileLink;

        let mut state = GameState::new_two_player(42);
        let source = create_object(
            &mut state,
            CardId(1),
            PlayerId(0),
            "Source".into(),
            Zone::Battlefield,
        );
        let exiled = create_object(
            &mut state,
            CardId(2),
            PlayerId(1),
            "Exiled Card".into(),
            Zone::Exile,
        );
        let unlinked = create_object(
            &mut state,
            CardId(3),
            PlayerId(1),
            "Other Card".into(),
            Zone::Exile,
        );

        // CR 610.3: ExileLink records which objects were exiled by which source.
        state.exile_links.push(ExileLink {
            exiled_id: exiled,
            source_id: source,
            return_zone: Zone::Battlefield,
        });

        let filter = TargetFilter::ExiledBySource;
        assert!(matches_target_filter(&state, exiled, &filter, source));
        assert!(
            !matches_target_filter(&state, unlinked, &filter, source),
            "unlinked object should not match ExiledBySource"
        );
    }

    #[test]
    fn shares_quality_creature_type_passes_with_shared_subtype() {
        let mut state = setup();
        let a = add_creature(&mut state, PlayerId(0), "Elf Warrior");
        state
            .objects
            .get_mut(&a)
            .unwrap()
            .card_types
            .subtypes
            .push("Elf".to_string());

        let b = add_creature(&mut state, PlayerId(0), "Elf Druid");
        state
            .objects
            .get_mut(&b)
            .unwrap()
            .card_types
            .subtypes
            .push("Elf".to_string());

        let targets = vec![TargetRef::Object(a), TargetRef::Object(b)];
        assert!(
            validate_shares_quality(&state, &targets, "creature type"),
            "Two Elves should share the Elf creature type"
        );
    }

    #[test]
    fn shares_quality_creature_type_fails_with_no_shared_subtype() {
        let mut state = setup();
        let a = add_creature(&mut state, PlayerId(0), "Elf");
        state
            .objects
            .get_mut(&a)
            .unwrap()
            .card_types
            .subtypes
            .push("Elf".to_string());

        let b = add_creature(&mut state, PlayerId(0), "Goblin");
        state
            .objects
            .get_mut(&b)
            .unwrap()
            .card_types
            .subtypes
            .push("Goblin".to_string());

        let targets = vec![TargetRef::Object(a), TargetRef::Object(b)];
        assert!(
            !validate_shares_quality(&state, &targets, "creature type"),
            "Elf and Goblin share no creature types"
        );
    }

    #[test]
    fn shares_quality_color_passes_with_shared_color() {
        let mut state = setup();
        let a = add_creature(&mut state, PlayerId(0), "Blue Red A");
        state.objects.get_mut(&a).unwrap().color = vec![ManaColor::Blue, ManaColor::Red];

        let b = add_creature(&mut state, PlayerId(0), "Blue Green B");
        state.objects.get_mut(&b).unwrap().color = vec![ManaColor::Blue, ManaColor::Green];

        let targets = vec![TargetRef::Object(a), TargetRef::Object(b)];
        assert!(
            validate_shares_quality(&state, &targets, "color"),
            "Both share Blue"
        );
    }

    #[test]
    fn shares_quality_color_fails_with_no_shared_color() {
        let mut state = setup();
        let a = add_creature(&mut state, PlayerId(0), "Red A");
        state.objects.get_mut(&a).unwrap().color = vec![ManaColor::Red];

        let b = add_creature(&mut state, PlayerId(0), "Blue B");
        state.objects.get_mut(&b).unwrap().color = vec![ManaColor::Blue];

        let targets = vec![TargetRef::Object(a), TargetRef::Object(b)];
        assert!(
            !validate_shares_quality(&state, &targets, "color"),
            "Red and Blue share no colors"
        );
    }

    #[test]
    fn attacked_this_turn_matches_tracked_creature() {
        let mut state = setup();
        let attacker = add_creature(&mut state, PlayerId(0), "Attacker");
        let bystander = add_creature(&mut state, PlayerId(0), "Bystander");
        state.creatures_attacked_this_turn.insert(attacker);

        let filter = TargetFilter::Typed(
            TypedFilter::creature().properties(vec![FilterProp::AttackedThisTurn]),
        );

        assert!(matches_target_filter(&state, attacker, &filter, attacker));
        assert!(!matches_target_filter(&state, bystander, &filter, attacker));
    }

    #[test]
    fn attacked_this_turn_works_post_combat() {
        let mut state = setup();
        let attacker = add_creature(&mut state, PlayerId(0), "Attacker");
        state.creatures_attacked_this_turn.insert(attacker);
        // combat is None post-combat — filter should still match via HashSet
        assert!(state.combat.is_none());

        let filter = TargetFilter::Typed(
            TypedFilter::creature().properties(vec![FilterProp::AttackedThisTurn]),
        );
        assert!(matches_target_filter(&state, attacker, &filter, attacker));
    }

    #[test]
    fn blocked_this_turn_matches_tracked_creature() {
        let mut state = setup();
        let blocker = add_creature(&mut state, PlayerId(1), "Blocker");
        let bystander = add_creature(&mut state, PlayerId(1), "Bystander");
        state.creatures_blocked_this_turn.insert(blocker);

        let filter = TargetFilter::Typed(
            TypedFilter::creature().properties(vec![FilterProp::BlockedThisTurn]),
        );

        assert!(matches_target_filter(&state, blocker, &filter, blocker));
        assert!(!matches_target_filter(&state, bystander, &filter, blocker));
    }

    #[test]
    fn attacked_or_blocked_this_turn_matches_either() {
        let mut state = setup();
        let attacker = add_creature(&mut state, PlayerId(0), "Attacker");
        let blocker = add_creature(&mut state, PlayerId(1), "Blocker");
        let neither = add_creature(&mut state, PlayerId(0), "Bystander");
        state.creatures_attacked_this_turn.insert(attacker);
        state.creatures_blocked_this_turn.insert(blocker);

        let filter = TargetFilter::Typed(
            TypedFilter::creature().properties(vec![FilterProp::AttackedOrBlockedThisTurn]),
        );

        assert!(matches_target_filter(&state, attacker, &filter, attacker));
        assert!(matches_target_filter(&state, blocker, &filter, attacker));
        assert!(!matches_target_filter(&state, neither, &filter, attacker));
    }
}
