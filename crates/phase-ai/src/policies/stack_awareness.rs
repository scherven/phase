use engine::types::ability::{Effect, QuantityExpr, TargetRef};
use engine::types::actions::GameAction;
use engine::types::game_state::GameState;
use engine::types::identifiers::ObjectId;
use engine::types::keywords::Keyword;
use engine::types::zones::Zone;

use super::context::{collect_ability_effects, PolicyContext};
use super::effect_classify::{effect_polarity, is_spell_beneficial, EffectPolarity};
use super::registry::TacticalPolicy;

pub struct StackAwarenessPolicy;

impl TacticalPolicy for StackAwarenessPolicy {
    fn score(&self, ctx: &PolicyContext<'_>) -> f64 {
        match &ctx.candidate.action {
            GameAction::ChooseTarget {
                target: Some(TargetRef::Object(id)),
            } => score_target_redundancy(ctx, *id),
            GameAction::SelectTargets { targets } => targets
                .iter()
                .map(|t| match t {
                    TargetRef::Object(id) => score_target_redundancy(ctx, *id),
                    _ => 0.0,
                })
                .sum(),
            _ => 0.0,
        }
    }
}

fn score_target_redundancy(ctx: &PolicyContext<'_>, target_id: ObjectId) -> f64 {
    if is_spell_beneficial(ctx) {
        return 0.0;
    }

    if !has_pending_removal(ctx.state, target_id) {
        return 0.0;
    }

    if will_target_die_from_stack(ctx.state, target_id) {
        ctx.penalties().redundant_removal_penalty
    } else {
        // Pending removal that might not kill — still penalize but less
        ctx.penalties().redundant_damage_penalty * 0.5
    }
}

/// Check if any stack entry targets this object with a harmful effect.
pub(crate) fn has_pending_removal(state: &GameState, target_id: ObjectId) -> bool {
    state.stack.iter().any(|entry| {
        let ability = entry.ability();
        let targets_this = ability
            .targets
            .iter()
            .any(|t| matches!(t, TargetRef::Object(id) if *id == target_id));
        if !targets_this {
            return false;
        }
        // Check if any effect in the chain is harmful
        collect_ability_effects(ability)
            .iter()
            .any(|e| matches!(effect_polarity(e), EffectPolarity::Harmful))
    })
}

/// Estimate whether pending stack effects will remove this creature from the battlefield.
pub(crate) fn will_target_die_from_stack(state: &GameState, target_id: ObjectId) -> bool {
    let Some(object) = state.objects.get(&target_id) else {
        return false;
    };

    let mut pending_damage: i32 = 0;

    for entry in state.stack.iter() {
        let ability = entry.ability();
        let targets_this = ability
            .targets
            .iter()
            .any(|t| matches!(t, TargetRef::Object(id) if *id == target_id));
        if !targets_this {
            continue;
        }

        for effect in collect_ability_effects(ability) {
            match effect {
                // Destroy is lethal unless target is indestructible
                Effect::Destroy { .. } if !object.has_keyword(&Keyword::Indestructible) => {
                    return true;
                }
                // Bounce removes from battlefield
                Effect::Bounce { .. } => return true,
                // ChangeZone to non-battlefield removes from battlefield
                Effect::ChangeZone {
                    destination: Zone::Exile | Zone::Graveyard | Zone::Hand | Zone::Library,
                    ..
                } => {
                    return true;
                }
                // Accumulate pending damage
                Effect::DealDamage {
                    amount: QuantityExpr::Fixed { value },
                    ..
                } => {
                    pending_damage += value;
                }
                _ => {}
            }
        }
    }

    // Check if accumulated pending damage is lethal
    if let Some(toughness) = object.toughness {
        let remaining = toughness - object.damage_marked as i32;
        pending_damage >= remaining
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AiConfig;
    use engine::ai_support::{ActionMetadata, AiDecisionContext, CandidateAction, TacticalClass};
    use engine::game::zones::create_object;
    use engine::types::ability::{ResolvedAbility, TargetFilter};
    use engine::types::card_type::CoreType;
    use engine::types::game_state::{
        GameState, PendingCast, StackEntry, StackEntryKind, TargetSelectionSlot, WaitingFor,
    };
    use engine::types::identifiers::{CardId, ObjectId};
    use engine::types::mana::ManaCost;
    use engine::types::player::PlayerId;
    use engine::types::zones::Zone;

    fn make_state() -> GameState {
        let mut state = GameState::new_two_player(42);
        state.turn_number = 2;
        state
    }

    fn add_creature(
        state: &mut GameState,
        owner: PlayerId,
        power: i32,
        toughness: i32,
    ) -> ObjectId {
        let id = create_object(
            state,
            CardId(state.next_object_id),
            owner,
            "Creature".to_string(),
            Zone::Battlefield,
        );
        let obj = state.objects.get_mut(&id).unwrap();
        obj.card_types.core_types.push(CoreType::Creature);
        obj.power = Some(power);
        obj.toughness = Some(toughness);
        id
    }

    fn push_stack_entry(state: &mut GameState, effect: Effect, targets: Vec<TargetRef>) {
        let ability = ResolvedAbility::new(effect, targets, ObjectId(999), PlayerId(1));
        state.stack.push(StackEntry {
            id: ObjectId(state.next_object_id),
            source_id: ObjectId(999),
            controller: PlayerId(1),
            kind: StackEntryKind::Spell {
                ability,
                card_id: CardId(999),
                casting_variant: Default::default(),
            },
        });
        state.next_object_id += 1;
    }

    fn make_target_ctx(
        _state: &GameState,
        target_id: ObjectId,
        source_effect: Effect,
    ) -> (AiDecisionContext, CandidateAction) {
        let ability = ResolvedAbility::new(source_effect, Vec::new(), ObjectId(888), PlayerId(1));
        let pending_cast = PendingCast::new(ObjectId(888), CardId(888), ability, ManaCost::zero());
        let decision = AiDecisionContext {
            waiting_for: WaitingFor::TargetSelection {
                player: PlayerId(1),
                pending_cast: Box::new(pending_cast),
                target_slots: vec![TargetSelectionSlot {
                    legal_targets: vec![TargetRef::Object(target_id)],
                    optional: false,
                }],
                selection: Default::default(),
            },
            candidates: Vec::new(),
        };
        let candidate = CandidateAction {
            action: GameAction::ChooseTarget {
                target: Some(TargetRef::Object(target_id)),
            },
            metadata: ActionMetadata {
                actor: Some(PlayerId(1)),
                tactical_class: TacticalClass::Target,
            },
        };
        (decision, candidate)
    }

    fn score_policy(
        state: &GameState,
        decision: &AiDecisionContext,
        candidate: &CandidateAction,
    ) -> f64 {
        let config = AiConfig::default();
        let ctx = PolicyContext {
            state,
            decision,
            candidate,
            ai_player: PlayerId(1),
            config: &config,
            context: &crate::context::AiContext::empty(&config.weights),
            cast_facts: None,
        };
        StackAwarenessPolicy.score(&ctx)
    }

    // --- Helper tests ---

    #[test]
    fn has_pending_removal_finds_destroy() {
        let mut state = make_state();
        let creature = add_creature(&mut state, PlayerId(0), 3, 3);
        push_stack_entry(
            &mut state,
            Effect::Destroy {
                target: TargetFilter::Any,
                cant_regenerate: false,
            },
            vec![TargetRef::Object(creature)],
        );
        assert!(has_pending_removal(&state, creature));
    }

    #[test]
    fn has_pending_removal_ignores_different_target() {
        let mut state = make_state();
        let creature_a = add_creature(&mut state, PlayerId(0), 3, 3);
        let creature_b = add_creature(&mut state, PlayerId(0), 2, 2);
        push_stack_entry(
            &mut state,
            Effect::Destroy {
                target: TargetFilter::Any,
                cant_regenerate: false,
            },
            vec![TargetRef::Object(creature_a)],
        );
        assert!(!has_pending_removal(&state, creature_b));
    }

    #[test]
    fn will_target_die_destroy() {
        let mut state = make_state();
        let creature = add_creature(&mut state, PlayerId(0), 3, 3);
        push_stack_entry(
            &mut state,
            Effect::Destroy {
                target: TargetFilter::Any,
                cant_regenerate: false,
            },
            vec![TargetRef::Object(creature)],
        );
        assert!(will_target_die_from_stack(&state, creature));
    }

    #[test]
    fn will_target_die_indestructible_survives_destroy() {
        let mut state = make_state();
        let creature = add_creature(&mut state, PlayerId(0), 3, 3);
        state
            .objects
            .get_mut(&creature)
            .unwrap()
            .keywords
            .push(Keyword::Indestructible);
        push_stack_entry(
            &mut state,
            Effect::Destroy {
                target: TargetFilter::Any,
                cant_regenerate: false,
            },
            vec![TargetRef::Object(creature)],
        );
        assert!(!will_target_die_from_stack(&state, creature));
    }

    #[test]
    fn will_target_die_lethal_damage() {
        let mut state = make_state();
        let creature = add_creature(&mut state, PlayerId(0), 2, 3);
        push_stack_entry(
            &mut state,
            Effect::DealDamage {
                amount: QuantityExpr::Fixed { value: 3 },
                target: TargetFilter::Any,
                damage_source: None,
            },
            vec![TargetRef::Object(creature)],
        );
        assert!(will_target_die_from_stack(&state, creature));
    }

    #[test]
    fn will_target_die_insufficient_damage() {
        let mut state = make_state();
        let creature = add_creature(&mut state, PlayerId(0), 2, 4);
        push_stack_entry(
            &mut state,
            Effect::DealDamage {
                amount: QuantityExpr::Fixed { value: 2 },
                target: TargetFilter::Any,
                damage_source: None,
            },
            vec![TargetRef::Object(creature)],
        );
        assert!(!will_target_die_from_stack(&state, creature));
    }

    #[test]
    fn will_target_die_bounce() {
        let mut state = make_state();
        let creature = add_creature(&mut state, PlayerId(0), 3, 3);
        push_stack_entry(
            &mut state,
            Effect::Bounce {
                target: TargetFilter::Any,
                destination: None,
            },
            vec![TargetRef::Object(creature)],
        );
        assert!(will_target_die_from_stack(&state, creature));
    }

    // --- Policy-level tests ---

    #[test]
    fn redundant_destroy_heavily_penalized() {
        let mut state = make_state();
        let creature = add_creature(&mut state, PlayerId(0), 3, 3);
        push_stack_entry(
            &mut state,
            Effect::Destroy {
                target: TargetFilter::Any,
                cant_regenerate: false,
            },
            vec![TargetRef::Object(creature)],
        );

        let (decision, candidate) = make_target_ctx(
            &state,
            creature,
            Effect::Destroy {
                target: TargetFilter::Any,
                cant_regenerate: false,
            },
        );
        let score = score_policy(&state, &decision, &candidate);
        assert!(
            score < -5.0,
            "Should heavily penalize redundant destroy, got {score}"
        );
    }

    #[test]
    fn no_penalty_when_different_targets() {
        let mut state = make_state();
        let creature_a = add_creature(&mut state, PlayerId(0), 3, 3);
        let creature_b = add_creature(&mut state, PlayerId(0), 2, 2);
        push_stack_entry(
            &mut state,
            Effect::Destroy {
                target: TargetFilter::Any,
                cant_regenerate: false,
            },
            vec![TargetRef::Object(creature_a)],
        );

        let (decision, candidate) = make_target_ctx(
            &state,
            creature_b,
            Effect::Destroy {
                target: TargetFilter::Any,
                cant_regenerate: false,
            },
        );
        let score = score_policy(&state, &decision, &candidate);
        assert!(
            score.abs() < 0.01,
            "No penalty when targeting different creature, got {score}"
        );
    }

    #[test]
    fn empty_stack_no_penalty() {
        let mut state = make_state();
        let creature = add_creature(&mut state, PlayerId(0), 3, 3);

        let (decision, candidate) = make_target_ctx(
            &state,
            creature,
            Effect::Destroy {
                target: TargetFilter::Any,
                cant_regenerate: false,
            },
        );
        let score = score_policy(&state, &decision, &candidate);
        assert!(
            score.abs() < 0.01,
            "No penalty with empty stack, got {score}"
        );
    }

    #[test]
    fn indestructible_not_penalized_second_removal() {
        let mut state = make_state();
        let creature = add_creature(&mut state, PlayerId(0), 3, 3);
        state
            .objects
            .get_mut(&creature)
            .unwrap()
            .keywords
            .push(Keyword::Indestructible);
        // First Destroy won't kill it (indestructible)
        push_stack_entry(
            &mut state,
            Effect::Destroy {
                target: TargetFilter::Any,
                cant_regenerate: false,
            },
            vec![TargetRef::Object(creature)],
        );

        // Second removal should still get partial penalty (there IS pending removal,
        // just not lethal)
        let (decision, candidate) = make_target_ctx(
            &state,
            creature,
            Effect::DealDamage {
                amount: QuantityExpr::Fixed { value: 5 },
                target: TargetFilter::Any,
                damage_source: None,
            },
        );
        let score = score_policy(&state, &decision, &candidate);
        // Should get partial penalty (redundant_damage * 0.5), not full redundant_removal
        assert!(
            score < 0.0 && score > -5.0,
            "Should get partial penalty for indestructible, got {score}"
        );
    }
}
