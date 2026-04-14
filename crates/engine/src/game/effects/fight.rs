use crate::game::ability_utils::append_to_sub_chain;
use crate::game::effects::append_to_pending_continuation;
use crate::game::effects::deal_damage::{apply_damage_to_target, DamageContext, DamageResult};
use crate::types::ability::{
    Effect, EffectError, EffectKind, QuantityExpr, ResolvedAbility, TargetFilter, TargetRef,
};
use crate::types::events::GameEvent;
use crate::types::game_state::GameState;
use crate::types::identifiers::ObjectId;

/// CR 701.14a: Resolve the subject creature for a fight effect.
/// - `SelfRef` → the ability's source object (default: "~ fights").
/// - `AttachedTo` → the permanent this Aura/Equipment is attached to
///   ("enchanted creature fights" / "equipped creature fights").
fn resolve_fight_subject(
    state: &GameState,
    ability: &ResolvedAbility,
) -> Result<ObjectId, EffectError> {
    let subject = match &ability.effect {
        Effect::Fight { subject, .. } => subject,
        _ => return Ok(ability.source_id),
    };
    if refers_to_attached(subject) {
        state
            .objects
            .get(&ability.source_id)
            .and_then(|obj| obj.attached_to)
            .ok_or_else(|| {
                EffectError::MissingParam("Fight subject: source not attached to anything".into())
            })
    } else {
        Ok(ability.source_id)
    }
}

/// Returns true if this filter refers to the permanent the source is attached to
/// (enchanted creature / equipped creature).
fn refers_to_attached(filter: &TargetFilter) -> bool {
    use crate::types::ability::FilterProp;
    matches!(filter, TargetFilter::AttachedTo)
        || matches!(filter, TargetFilter::Typed(tf) if tf.properties.iter().any(|p|
            matches!(p, FilterProp::EnchantedBy | FilterProp::EquippedBy)
        ))
}

/// CR 701.14a: Fight — each creature deals damage equal to its power to the other.
pub fn resolve(
    state: &mut GameState,
    ability: &ResolvedAbility,
    events: &mut Vec<GameEvent>,
) -> Result<(), EffectError> {
    // CR 701.14a: Resolve the fighting creature from the effect's subject.
    // For "enchanted creature fights", subject is AttachedTo → look up attached_to.
    // For "~ fights", subject is SelfRef → use ability.source_id directly.
    let source_id = resolve_fight_subject(state, ability)?;

    // Target creature from ability.targets
    let target_id = ability
        .targets
        .iter()
        .find_map(|t| {
            if let TargetRef::Object(id) = t {
                Some(*id)
            } else {
                None
            }
        })
        .ok_or_else(|| EffectError::MissingParam("Fight target".to_string()))?;

    // Read power and controller for both creatures before mutable damage phase.
    let (source_power, source_controller) = {
        let obj = state
            .objects
            .get(&source_id)
            .ok_or(EffectError::ObjectNotFound(source_id))?;
        (obj.power.unwrap_or(0), obj.controller)
    };
    let (target_power, target_controller) = {
        let obj = state
            .objects
            .get(&target_id)
            .ok_or(EffectError::ObjectNotFound(target_id))?;
        (obj.power.unwrap_or(0), obj.controller)
    };

    // CR 701.14a + CR 120.2b: Fight damage is not combat damage.
    // Source deals damage to target (power of source → target's damage)
    if source_power > 0 {
        let source_ctx = DamageContext::from_source(state, source_id)
            .unwrap_or_else(|| DamageContext::fallback(source_id, source_controller));
        if let DamageResult::NeedsChoice = apply_damage_to_target(
            state,
            &source_ctx,
            TargetRef::Object(target_id),
            source_power as u32,
            false,
            events,
        )? {
            // CR 701.14a: First direction is waiting on a replacement choice.
            // Stash a continuation so the second direction (target → source) resumes
            // after the choice resolves. The parent's sub_ability (if any) is appended
            // to the continuation's tail so downstream effects still fire.
            if target_power > 0 {
                // Second direction: target_id (the fight target) deals damage equal
                // to its power to source_id (the fighter).
                let mut second = build_fight_damage_node(
                    target_id,
                    source_id,
                    target_power as u32,
                    target_controller,
                );
                if let Some(sub) = ability.sub_ability.as_ref() {
                    append_to_sub_chain(&mut second, sub.as_ref().clone());
                }
                append_to_pending_continuation(state, Some(Box::new(second)));
            } else if let Some(sub) = ability.sub_ability.as_ref() {
                append_to_pending_continuation(state, Some(Box::new(sub.as_ref().clone())));
            }
            return Ok(());
        }
    }

    // Target deals damage to source (power of target → source's damage)
    if target_power > 0 {
        let target_ctx = DamageContext::from_source(state, target_id)
            .unwrap_or_else(|| DamageContext::fallback(target_id, target_controller));
        if let DamageResult::NeedsChoice = apply_damage_to_target(
            state,
            &target_ctx,
            TargetRef::Object(source_id),
            target_power as u32,
            false,
            events,
        )? {
            // CR 701.14a: Second direction is waiting on a replacement choice — no more
            // damage to deal, but propagate the parent's sub_ability if present.
            if let Some(sub) = ability.sub_ability.as_ref() {
                append_to_pending_continuation(state, Some(Box::new(sub.as_ref().clone())));
            }
            return Ok(());
        }
    }

    events.push(GameEvent::EffectResolved {
        kind: EffectKind::from(&ability.effect),
        source_id: ability.source_id,
    });

    Ok(())
}

/// CR 701.14a: Build a one-shot, single-target non-combat `DealDamage` node representing
/// one direction of a fight. `source_id` deals `amount` damage to `target_id`.
/// Used as a continuation when the first direction of fight damage pauses for a
/// replacement choice.
fn build_fight_damage_node(
    source_id: ObjectId,
    target_id: ObjectId,
    amount: u32,
    controller: crate::types::player::PlayerId,
) -> ResolvedAbility {
    ResolvedAbility::new(
        Effect::DealDamage {
            amount: QuantityExpr::Fixed {
                value: amount as i32,
            },
            target: TargetFilter::Any,
            damage_source: None,
        },
        vec![TargetRef::Object(target_id)],
        source_id,
        controller,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::zones::create_object;
    use crate::types::ability::{Effect, TargetFilter};
    use crate::types::card_type::CoreType;
    use crate::types::identifiers::{CardId, ObjectId};
    use crate::types::player::PlayerId;
    use crate::types::zones::Zone;

    fn make_creature(
        state: &mut GameState,
        owner: PlayerId,
        name: &str,
        power: i32,
        toughness: i32,
    ) -> ObjectId {
        let id = create_object(
            state,
            CardId(state.next_object_id),
            owner,
            name.to_string(),
            Zone::Battlefield,
        );
        let obj = state.objects.get_mut(&id).unwrap();
        obj.power = Some(power);
        obj.toughness = Some(toughness);
        obj.base_power = Some(power);
        obj.base_toughness = Some(toughness);
        obj.card_types.core_types.push(CoreType::Creature);
        id
    }

    fn make_fight_ability(source: ObjectId, target: ObjectId) -> ResolvedAbility {
        ResolvedAbility::new(
            Effect::Fight {
                target: TargetFilter::Any,
                subject: TargetFilter::SelfRef,
            },
            vec![TargetRef::Object(target)],
            source,
            PlayerId(0),
        )
    }

    #[test]
    fn test_fight_mutual_damage() {
        let mut state = GameState::new_two_player(42);
        let bear = make_creature(&mut state, PlayerId(0), "Bear", 3, 3);
        let wolf = make_creature(&mut state, PlayerId(1), "Wolf", 2, 2);

        let ability = make_fight_ability(bear, wolf);
        let mut events = Vec::new();

        resolve(&mut state, &ability, &mut events).unwrap();

        // Bear (3/3) deals 3 damage to Wolf -> Wolf has 3 damage
        assert_eq!(state.objects[&wolf].damage_marked, 3);
        // Wolf (2/2) deals 2 damage to Bear -> Bear has 2 damage
        assert_eq!(state.objects[&bear].damage_marked, 2);
    }

    #[test]
    fn test_fight_emits_damage_events() {
        let mut state = GameState::new_two_player(42);
        let bear = make_creature(&mut state, PlayerId(0), "Bear", 3, 3);
        let wolf = make_creature(&mut state, PlayerId(1), "Wolf", 2, 2);

        let ability = make_fight_ability(bear, wolf);
        let mut events = Vec::new();

        resolve(&mut state, &ability, &mut events).unwrap();

        // Should have 2 DamageDealt events + 1 EffectResolved
        let damage_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, GameEvent::DamageDealt { .. }))
            .collect();
        assert_eq!(damage_events.len(), 2);
    }

    #[test]
    fn test_fight_zero_power_no_damage() {
        let mut state = GameState::new_two_player(42);
        let wall = make_creature(&mut state, PlayerId(0), "Wall", 0, 5);
        let bear = make_creature(&mut state, PlayerId(1), "Bear", 2, 2);

        let ability = make_fight_ability(wall, bear);
        let mut events = Vec::new();

        resolve(&mut state, &ability, &mut events).unwrap();

        // Wall has 0 power, deals no damage to Bear
        assert_eq!(state.objects[&bear].damage_marked, 0);
        // Bear has 2 power, deals 2 damage to Wall
        assert_eq!(state.objects[&wall].damage_marked, 2);
    }

    #[test]
    fn fight_lifelink_gains_life() {
        let mut state = GameState::new_two_player(42);
        let lifelinker = make_creature(&mut state, PlayerId(0), "Lifelinker", 3, 3);
        state
            .objects
            .get_mut(&lifelinker)
            .unwrap()
            .keywords
            .push(crate::types::keywords::Keyword::Lifelink);
        let wolf = make_creature(&mut state, PlayerId(1), "Wolf", 2, 2);

        let ability = make_fight_ability(lifelinker, wolf);
        let mut events = Vec::new();

        resolve(&mut state, &ability, &mut events).unwrap();

        // CR 702.15b: Lifelink — controller gains life equal to damage dealt.
        assert_eq!(state.objects[&wolf].damage_marked, 3);
        assert_eq!(state.objects[&lifelinker].damage_marked, 2);
        assert_eq!(state.players[0].life, 23); // 20 + 3 from lifelink
        assert_eq!(state.players[1].life, 20); // unchanged (no lifelink)
    }

    #[test]
    fn fight_aura_enchanted_creature_is_subject() {
        // "Enchanted creature fights target creature" — the Aura is the source,
        // but the enchanted creature should be the fighter, not the Aura.
        let mut state = GameState::new_two_player(42);
        let bear = make_creature(&mut state, PlayerId(0), "Bear", 3, 3);
        let wolf = make_creature(&mut state, PlayerId(1), "Wolf", 2, 2);

        // Create an Aura attached to the bear
        let aura_card_id = CardId(state.next_object_id);
        let aura_id = create_object(
            &mut state,
            aura_card_id,
            PlayerId(0),
            "Test Aura".to_string(),
            Zone::Battlefield,
        );
        let aura = state.objects.get_mut(&aura_id).unwrap();
        aura.card_types.core_types.push(CoreType::Enchantment);
        aura.attached_to = Some(bear);

        // Fight with subject = enchanted creature (Typed filter with EnchantedBy)
        let ability = ResolvedAbility::new(
            Effect::Fight {
                target: TargetFilter::Any,
                subject: TargetFilter::Typed(
                    crate::types::ability::TypedFilter::creature()
                        .properties(vec![crate::types::ability::FilterProp::EnchantedBy]),
                ),
            },
            vec![TargetRef::Object(wolf)],
            aura_id,
            PlayerId(0),
        );
        let mut events = Vec::new();

        resolve(&mut state, &ability, &mut events).unwrap();

        // Bear (3/3) should fight Wolf (2/2), not the Aura
        assert_eq!(state.objects[&wolf].damage_marked, 3);
        assert_eq!(state.objects[&bear].damage_marked, 2);
    }

    /// CR 120.3 + CR 616.1e: When the first direction of fight damage pauses on a
    /// replacement choice, the second direction must be stashed as
    /// `pending_continuation` so it resumes after the choice is answered — not
    /// silently vanish.
    ///
    /// NOTE: This verifies the continuation structure only. End-to-end resolution
    /// through `handle_replacement_choice` for Damage events is tracked separately
    /// — that handler currently only re-applies ZoneChange results post-choice.
    #[test]
    fn fight_with_damage_replacement_on_first_direction() {
        use crate::game::game_object::GameObject;
        use crate::types::ability::{ReplacementDefinition, ReplacementMode};
        use crate::types::game_state::WaitingFor;
        use crate::types::replacements::ReplacementEvent;

        let mut state = GameState::new_two_player(42);
        let bear = make_creature(&mut state, PlayerId(0), "Bear", 3, 3);
        let wolf = make_creature(&mut state, PlayerId(1), "Wolf", 2, 2);

        // Install an Optional DamageDone replacement on a host object so the first
        // damage event (bear → wolf) pauses for a player choice.
        let shield_id = ObjectId(state.next_object_id);
        state.next_object_id += 1;
        let mut shield = GameObject::new(
            shield_id,
            CardId(99),
            PlayerId(1),
            "Shield".to_string(),
            Zone::Battlefield,
        );
        shield.replacement_definitions.push(
            ReplacementDefinition::new(ReplacementEvent::DamageDone)
                .mode(ReplacementMode::Optional { decline: None })
                .description("Shield".to_string()),
        );
        state.objects.insert(shield_id, shield);
        state.battlefield.push(shield_id);

        let ability = make_fight_ability(bear, wolf);
        let mut events = Vec::new();
        resolve(&mut state, &ability, &mut events).unwrap();

        // First direction paused on the replacement choice.
        assert!(matches!(
            state.waiting_for,
            WaitingFor::ReplacementChoice { .. }
        ));
        // A continuation was stashed for the second direction — previously this
        // branch silently returned Ok(()) and the second direction was dropped.
        let cont = state
            .pending_continuation
            .as_ref()
            .expect("expected pending_continuation for second-direction fight damage");
        // Continuation is a single-target DealDamage from wolf to bear.
        match &cont.effect {
            Effect::DealDamage {
                amount: QuantityExpr::Fixed { value },
                ..
            } => {
                assert_eq!(*value, 2, "wolf.power = 2 should drive second direction");
            }
            other => panic!("expected DealDamage continuation, got {other:?}"),
        }
        assert_eq!(
            cont.source_id, wolf,
            "wolf deals the second-direction damage"
        );
        assert_eq!(cont.targets, vec![TargetRef::Object(bear)]);
        // Bear hasn't taken damage yet — second direction is still pending.
        assert_eq!(state.objects[&bear].damage_marked, 0);
    }
}
