use std::str::FromStr;

use crate::types::ability::{
    ContinuousModification, Duration, Effect, EffectError, EffectKind, ResolvedAbility,
    TargetFilter, TargetRef,
};
use crate::types::card_type::CoreType;
use crate::types::events::{BendingType, GameEvent};
use crate::types::game_state::GameState;

/// CR 613.1: Animation — apply type/subtype and P/T changes via the layer system.
/// Uses `TransientContinuousEffect` so the layer system handles ordering (CR 613.1d,
/// CR 613.1f, CR 613.4) and automatic cleanup at end-of-turn or when source leaves play.
pub fn resolve(
    state: &mut GameState,
    ability: &ResolvedAbility,
    events: &mut Vec<GameEvent>,
) -> Result<(), EffectError> {
    let (power, toughness, types_list, remove_types_list, kw_list, is_earthbend) =
        match &ability.effect {
            Effect::Animate {
                power,
                toughness,
                types,
                remove_types,
                keywords,
                is_earthbend,
                ..
            } => (
                *power,
                *toughness,
                types.as_slice(),
                remove_types.as_slice(),
                keywords.as_slice(),
                *is_earthbend,
            ),
            _ => (
                None,
                None,
                [].as_slice(),
                [].as_slice(),
                [].as_slice(),
                false,
            ),
        };

    let targets = resolve_animate_targets(ability);

    // Determine duration from the ability definition's duration field.
    let duration = ability.duration.clone().unwrap_or(Duration::UntilEndOfTurn);

    // CR 613.1: Build layer-appropriate modifications instead of direct mutation.
    let mut modifications = Vec::new();

    // CR 613.4 / Layer 7b: Set base P/T (overrides printed values).
    if let Some(p) = power {
        modifications.push(ContinuousModification::SetPower { value: p });
    }
    if let Some(t) = toughness {
        modifications.push(ContinuousModification::SetToughness { value: t });
    }

    // CR 613.1d / Layer 4: Add types and subtypes.
    for t in types_list {
        let t = t.trim();
        if let Ok(core) = CoreType::from_str(t) {
            modifications.push(ContinuousModification::AddType { core_type: core });
        } else {
            modifications.push(ContinuousModification::AddSubtype {
                subtype: t.to_string(),
            });
        }
    }

    // CR 205.1a / Layer 4: Remove core types (e.g., Glimmer cycle "it's not a creature").
    for t in remove_types_list {
        if let Ok(core) = CoreType::from_str(t.trim()) {
            modifications.push(ContinuousModification::RemoveType { core_type: core });
        }
    }

    // CR 613.1f / Layer 6: Add keywords.
    for kw in kw_list {
        modifications.push(ContinuousModification::AddKeyword {
            keyword: kw.clone(),
        });
    }

    // Register a TransientContinuousEffect per target so the layer system handles
    // ordering and cleanup automatically.
    for obj_id in targets {
        if !state.objects.contains_key(&obj_id) {
            return Err(EffectError::ObjectNotFound(obj_id));
        }
        state.add_transient_continuous_effect(
            ability.source_id,
            ability.controller,
            duration.clone(),
            TargetFilter::SpecificObject { id: obj_id },
            modifications.clone(),
            None,
        );
    }

    // Emit earthbend event for bending trigger system (mirrors grant_permission.rs Airbend pattern)
    if is_earthbend {
        events.push(GameEvent::Earthbend {
            source_id: ability.source_id,
            controller: ability.controller,
        });
        if let Some(p) = state
            .players
            .iter_mut()
            .find(|p| p.id == ability.controller)
        {
            p.bending_types_this_turn.insert(BendingType::Earth);
        }
    }

    events.push(GameEvent::EffectResolved {
        kind: EffectKind::from(&ability.effect),
        source_id: ability.source_id,
    });

    Ok(())
}

fn resolve_animate_targets(ability: &ResolvedAbility) -> Vec<crate::types::identifiers::ObjectId> {
    if let Effect::Animate { target, .. } = &ability.effect {
        if matches!(target, TargetFilter::None) {
            return vec![ability.source_id];
        }
    }
    ability
        .targets
        .iter()
        .filter_map(|t| {
            if let TargetRef::Object(id) = t {
                Some(*id)
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::zones::create_object;
    use crate::types::identifiers::CardId;
    use crate::types::player::PlayerId;
    use crate::types::zones::Zone;

    #[test]
    fn animate_creates_transient_continuous_effect() {
        let mut state = GameState::new_two_player(42);
        let obj_id = create_object(
            &mut state,
            CardId(1),
            PlayerId(0),
            "Enchantment".to_string(),
            Zone::Battlefield,
        );

        let ability = ResolvedAbility::new(
            Effect::Animate {
                power: Some(7),
                toughness: Some(7),
                types: vec!["Creature".to_string(), "Beast".to_string()],
                remove_types: vec![],
                keywords: vec![],
                target: TargetFilter::None,
                is_earthbend: false,
            },
            vec![],
            obj_id,
            PlayerId(0),
        );

        let mut events = Vec::new();
        resolve(&mut state, &ability, &mut events).unwrap();

        // Should create a TransientContinuousEffect instead of mutating directly
        assert_eq!(state.transient_continuous_effects.len(), 1);
        let tce = &state.transient_continuous_effects[0];
        assert_eq!(tce.affected, TargetFilter::SpecificObject { id: obj_id });
        assert!(tce
            .modifications
            .contains(&ContinuousModification::SetPower { value: 7 }));
        assert!(tce
            .modifications
            .contains(&ContinuousModification::SetToughness { value: 7 }));
        assert!(tce
            .modifications
            .contains(&ContinuousModification::AddType {
                core_type: CoreType::Creature
            }));
        assert!(tce
            .modifications
            .contains(&ContinuousModification::AddSubtype {
                subtype: "Beast".to_string()
            }));
    }

    #[test]
    fn animate_uses_until_end_of_turn_by_default() {
        let mut state = GameState::new_two_player(42);
        let obj_id = create_object(
            &mut state,
            CardId(1),
            PlayerId(0),
            "Land".to_string(),
            Zone::Battlefield,
        );

        let ability = ResolvedAbility::new(
            Effect::Animate {
                power: Some(3),
                toughness: Some(3),
                types: vec!["Creature".to_string()],
                remove_types: vec![],
                keywords: vec![],
                target: TargetFilter::None,
                is_earthbend: false,
            },
            vec![],
            obj_id,
            PlayerId(0),
        );

        let mut events = Vec::new();
        resolve(&mut state, &ability, &mut events).unwrap();

        assert_eq!(
            state.transient_continuous_effects[0].duration,
            Duration::UntilEndOfTurn
        );
    }

    #[test]
    fn animate_respects_explicit_duration() {
        let mut state = GameState::new_two_player(42);
        let obj_id = create_object(
            &mut state,
            CardId(1),
            PlayerId(0),
            "Artifact".to_string(),
            Zone::Battlefield,
        );

        let mut ability = ResolvedAbility::new(
            Effect::Animate {
                power: Some(5),
                toughness: Some(5),
                types: vec!["Creature".to_string()],
                remove_types: vec![],
                keywords: vec![],
                target: TargetFilter::None,
                is_earthbend: false,
            },
            vec![],
            obj_id,
            PlayerId(0),
        );
        ability.duration = Some(Duration::UntilHostLeavesPlay);

        let mut events = Vec::new();
        resolve(&mut state, &ability, &mut events).unwrap();

        assert_eq!(
            state.transient_continuous_effects[0].duration,
            Duration::UntilHostLeavesPlay
        );
    }
}
