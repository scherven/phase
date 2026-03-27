use crate::types::ability::{
    CombatDamageScope, DamageTargetFilter, Effect, EffectError, EffectKind, PreventionScope,
    ReplacementDefinition, ResolvedAbility, TargetFilter, TargetRef,
};
use crate::types::events::GameEvent;
use crate::types::game_state::GameState;
use crate::types::replacements::ReplacementEvent;
use crate::types::zones::Zone;

/// CR 615: Prevent damage — creates a prevention shield on the source object.
///
/// The shield is stored as a `ReplacementDefinition` with `ShieldKind::Prevention`
/// on the source object's `replacement_definitions`. The `damage_done_applier`
/// in `replacement.rs` consumes these shields when matching `ProposedEvent::Damage`.
///
/// Follows the same lifecycle as regeneration shields:
/// 1. Created here → 2. Matched/applied in replacement pipeline → 3. Cleaned up at end of turn
pub fn resolve(
    state: &mut GameState,
    ability: &ResolvedAbility,
    events: &mut Vec<GameEvent>,
) -> Result<(), EffectError> {
    let (amount, scope) = match &ability.effect {
        Effect::PreventDamage { amount, scope, .. } => (*amount, *scope),
        _ => {
            return Err(EffectError::InvalidParam(
                "expected PreventDamage effect".to_string(),
            ))
        }
    };

    // Build the prevention shield replacement definition.
    let mut shield = ReplacementDefinition::new(ReplacementEvent::DamageDone)
        .prevention_shield(amount)
        .valid_card(TargetFilter::SelfRef)
        .description("Prevent damage".to_string());

    // CR 615: Scope restriction — combat damage only vs all damage
    if scope == PreventionScope::CombatDamage {
        shield = shield.combat_scope(CombatDamageScope::CombatOnly);
    }

    // CR 615: For targeted prevention ("prevent the next N damage to target creature"),
    // the shield lives on the TARGET object — same pattern as regeneration shields.
    // This ensures the shield is found by find_applicable_replacements() which only
    // scans Battlefield/Command zones (instants move to graveyard after resolving).
    //
    // For untargeted effects (Fog: "prevent all combat damage"), the shield lives on
    // the source permanent. If the source is an instant/sorcery, the shield won't persist
    // after resolution — untargeted instant prevention requires a global mechanism (future work).
    if !ability.targets.is_empty() {
        for target in &ability.targets {
            match target {
                TargetRef::Object(obj_id) => {
                    if let Some(obj) = state.objects.get_mut(obj_id) {
                        obj.replacement_definitions.push(shield.clone());
                    }
                }
                TargetRef::Player(_) => {
                    // Player-targeted prevention: attach to source (permanent abilities)
                    // and scope with damage_target_filter.
                    let player_shield = shield
                        .clone()
                        .damage_target_filter(DamageTargetFilter::PlayerOnly);
                    if let Some(obj) = state.objects.get_mut(&ability.source_id) {
                        obj.replacement_definitions.push(player_shield);
                    }
                }
            }
        }
    } else {
        // CR 615.3: Untargeted prevention (e.g., Fog): check if source is a permanent
        // on the battlefield. If so, attach to source. If source is an instant/sorcery
        // (will move to graveyard on resolution), use the game-state-level registry.
        let is_on_battlefield = state
            .objects
            .get(&ability.source_id)
            .is_some_and(|obj| obj.zone == Zone::Battlefield || obj.zone == Zone::Stack);
        if is_on_battlefield {
            if let Some(obj) = state.objects.get_mut(&ability.source_id) {
                obj.replacement_definitions.push(shield);
            }
        } else {
            // Source left the battlefield (instant/sorcery resolved) — store globally.
            state.pending_damage_prevention.push(shield);
        }
    }

    events.push(GameEvent::EffectResolved {
        kind: EffectKind::PreventDamage,
        source_id: ability.source_id,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::zones::create_object;
    use crate::types::ability::{PreventionAmount, ShieldKind};
    use crate::types::identifiers::{CardId, ObjectId};
    use crate::types::player::PlayerId;
    use crate::types::zones::Zone;

    fn make_prevent_ability(
        source: ObjectId,
        amount: PreventionAmount,
        scope: PreventionScope,
        targets: Vec<TargetRef>,
    ) -> ResolvedAbility {
        ResolvedAbility::new(
            Effect::PreventDamage {
                amount,
                target: TargetFilter::Any,
                scope,
            },
            targets,
            source,
            PlayerId(0),
        )
    }

    #[test]
    fn prevent_all_creates_shield_on_source() {
        let mut state = GameState::new_two_player(42);
        let source = create_object(
            &mut state,
            CardId(1),
            PlayerId(0),
            "Fog".to_string(),
            Zone::Battlefield,
        );

        let ability = make_prevent_ability(
            source,
            PreventionAmount::All,
            PreventionScope::AllDamage,
            vec![],
        );
        let mut events = Vec::new();
        resolve(&mut state, &ability, &mut events).unwrap();

        let obj = state.objects.get(&source).unwrap();
        assert_eq!(obj.replacement_definitions.len(), 1);
        assert!(matches!(
            obj.replacement_definitions[0].shield_kind,
            ShieldKind::Prevention {
                amount: PreventionAmount::All
            }
        ));
        assert_eq!(
            obj.replacement_definitions[0].event,
            ReplacementEvent::DamageDone
        );
        assert!(!obj.replacement_definitions[0].is_consumed);
    }

    #[test]
    fn prevent_next_n_creates_shield_with_amount() {
        let mut state = GameState::new_two_player(42);
        let source = create_object(
            &mut state,
            CardId(1),
            PlayerId(0),
            "Shield".to_string(),
            Zone::Battlefield,
        );

        let ability = make_prevent_ability(
            source,
            PreventionAmount::Next(3),
            PreventionScope::AllDamage,
            vec![],
        );
        let mut events = Vec::new();
        resolve(&mut state, &ability, &mut events).unwrap();

        let obj = state.objects.get(&source).unwrap();
        assert!(matches!(
            obj.replacement_definitions[0].shield_kind,
            ShieldKind::Prevention {
                amount: PreventionAmount::Next(3)
            }
        ));
    }

    #[test]
    fn combat_damage_scope_sets_combat_only() {
        let mut state = GameState::new_two_player(42);
        let source = create_object(
            &mut state,
            CardId(1),
            PlayerId(0),
            "Fog".to_string(),
            Zone::Battlefield,
        );

        let ability = make_prevent_ability(
            source,
            PreventionAmount::All,
            PreventionScope::CombatDamage,
            vec![],
        );
        let mut events = Vec::new();
        resolve(&mut state, &ability, &mut events).unwrap();

        let obj = state.objects.get(&source).unwrap();
        assert_eq!(
            obj.replacement_definitions[0].combat_scope,
            Some(CombatDamageScope::CombatOnly)
        );
    }

    #[test]
    fn emits_effect_resolved() {
        let mut state = GameState::new_two_player(42);
        let source = create_object(
            &mut state,
            CardId(1),
            PlayerId(0),
            "Fog".to_string(),
            Zone::Battlefield,
        );

        let ability = make_prevent_ability(
            source,
            PreventionAmount::All,
            PreventionScope::AllDamage,
            vec![],
        );
        let mut events = Vec::new();
        resolve(&mut state, &ability, &mut events).unwrap();

        assert!(events.iter().any(|e| matches!(
            e,
            GameEvent::EffectResolved {
                kind: EffectKind::PreventDamage,
                ..
            }
        )));
    }
}
