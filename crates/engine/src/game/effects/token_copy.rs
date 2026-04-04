use crate::game::layers::compute_current_copiable_values;
use crate::game::zones;
use crate::types::ability::{
    Effect, EffectError, EffectKind, ResolvedAbility, TargetFilter, TargetRef,
};
use crate::types::events::GameEvent;
use crate::types::game_state::GameState;
use crate::types::identifiers::CardId;
use crate::types::zones::Zone;

/// CR 707.2 / CR 707.5: Create a token that's a copy of a permanent.
/// Copies copiable characteristics from the target to a newly created token.
pub fn resolve(
    state: &mut GameState,
    ability: &ResolvedAbility,
    events: &mut Vec<GameEvent>,
) -> Result<(), EffectError> {
    // Extract fields from effect
    let (target_filter, enters_attacking, tapped) = match &ability.effect {
        Effect::CopyTokenOf {
            target,
            enters_attacking,
            tapped,
        } => (target, *enters_attacking, *tapped),
        _ => return Err(EffectError::MissingParam("CopyTokenOf".to_string())),
    };

    // Step 1: Resolve the copy source.
    // SelfRef with no explicit targets → copy the source permanent itself.
    // Otherwise → use the first Object target from ability.targets.
    let copy_source_id = if matches!(target_filter, TargetFilter::SelfRef)
        && ability.targets.is_empty()
    {
        ability.source_id
    } else {
        ability
            .targets
            .iter()
            .find_map(|t| match t {
                TargetRef::Object(id) => Some(*id),
                _ => None,
            })
            .ok_or_else(|| EffectError::MissingParam("CopyTokenOf requires a target".to_string()))?
    };

    let values = compute_current_copiable_values(state, copy_source_id)
        .ok_or(EffectError::ObjectNotFound(copy_source_id))?;
    let name = values.name.clone();

    // Step 3: Create a new token object on the battlefield.
    let token_id = zones::create_object(
        state,
        CardId(0),
        ability.controller,
        name.clone(),
        Zone::Battlefield,
    );

    // Step 4: Apply snapshotted characteristics to the token (CR 707.2).
    let token = state.objects.get_mut(&token_id).unwrap();
    token.is_token = true;
    token.name = values.name.clone();
    token.base_name = values.name.clone();
    token.mana_cost = values.mana_cost.clone();
    token.base_mana_cost = values.mana_cost.clone();
    token.base_color = values.color.clone();
    token.color = values.color.clone();
    token.base_card_types = values.card_types.clone();
    token.card_types = values.card_types.clone();
    token.base_power = values.power;
    token.power = values.power;
    token.base_toughness = values.toughness;
    token.toughness = values.toughness;
    token.base_loyalty = values.loyalty;
    token.loyalty = values.loyalty;
    token.base_keywords = values.keywords.clone();
    token.keywords = values.keywords.clone();
    token.base_abilities = values.abilities.clone();
    token.abilities = values.abilities.clone();
    token.base_trigger_definitions = values.trigger_definitions.clone();
    token.trigger_definitions = values.trigger_definitions.clone();
    token.base_replacement_definitions = values.replacement_definitions.clone();
    token.replacement_definitions = values.replacement_definitions.clone();
    token.base_static_definitions = values.static_definitions.clone();
    token.static_definitions = values.static_definitions.clone();
    token.base_characteristics_initialized = true;
    token.entered_battlefield_turn = Some(state.turn_number);

    // Step 5: If tapped, set tapped state.
    if tapped {
        token.tapped = true;
    }

    // Step 6: If enters_attacking, add to combat attackers.
    // CR 508.4: Uses shared helper for defending player resolution.
    if enters_attacking {
        crate::game::combat::enter_attacking(
            state,
            token_id,
            ability.source_id,
            ability.controller,
        );
    }

    // Step 6b: Inject predefined abilities, record entry, and mark layers dirty.
    // CR 111.10a-v: Predefined token abilities for known subtypes (Treasure, Food, etc.).
    super::token::inject_predefined_token_abilities(state, token_id);
    state.layers_dirty = true;
    crate::game::restrictions::record_battlefield_entry(state, token_id);
    crate::game::restrictions::record_token_created(state, token_id);

    // Step 7: Emit events.
    events.push(GameEvent::TokenCreated {
        object_id: token_id,
        name,
    });
    events.push(GameEvent::EffectResolved {
        kind: EffectKind::from(&ability.effect),
        source_id: ability.source_id,
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::zones::create_object;
    use crate::types::ability::{Effect, TargetFilter, TargetRef};
    use crate::types::card_type::{CardType, CoreType};
    use crate::types::identifiers::ObjectId;
    use crate::types::keywords::Keyword;
    use crate::types::mana::ManaColor;
    use crate::types::player::PlayerId;

    #[test]
    fn copy_token_of_self_creates_copy() {
        let mut state = GameState::new_two_player(42);

        // Create a creature to copy
        let source_id = create_object(
            &mut state,
            CardId(1),
            PlayerId(0),
            "Mist-Syndicate Naga".to_string(),
            Zone::Battlefield,
        );
        {
            let source = state.objects.get_mut(&source_id).unwrap();
            source.base_power = Some(3);
            source.base_toughness = Some(1);
            source.power = Some(3);
            source.toughness = Some(1);
            source.base_color = vec![ManaColor::Blue];
            source.color = vec![ManaColor::Blue];
            source.base_card_types = CardType {
                supertypes: vec![],
                core_types: vec![CoreType::Creature],
                subtypes: vec!["Snake".to_string(), "Ninja".to_string()],
            };
            source.card_types = source.base_card_types.clone();
            source.base_keywords = vec![Keyword::Ninjutsu(Default::default())];
            source.keywords = source.base_keywords.clone();
        }

        let mut events = Vec::new();
        let ability = ResolvedAbility::new(
            Effect::CopyTokenOf {
                target: TargetFilter::SelfRef,
                enters_attacking: false,
                tapped: false,
            },
            vec![],
            source_id,
            PlayerId(0),
        );

        resolve(&mut state, &ability, &mut events).unwrap();

        // Find the token (it's the newest object)
        let token_id = ObjectId(state.next_object_id - 1);
        let token = state.objects.get(&token_id).unwrap();

        assert_eq!(token.name, "Mist-Syndicate Naga");
        assert_eq!(token.power, Some(3));
        assert_eq!(token.toughness, Some(1));
        assert_eq!(token.color, vec![ManaColor::Blue]);
        assert!(token.card_types.core_types.contains(&CoreType::Creature));
        assert!(token.card_types.subtypes.contains(&"Snake".to_string()));
        assert!(token.is_token);
        assert!(token.zone == Zone::Battlefield);
        assert!(state.layers_dirty);
        assert!(events.iter().any(
            |e| matches!(e, GameEvent::TokenCreated { name, .. } if name == "Mist-Syndicate Naga")
        ));
        // Verify record_battlefield_entry and record_token_created were called
        assert!(
            state
                .players_who_created_token_this_turn
                .contains(&PlayerId(0)),
            "should record token creation"
        );
    }

    #[test]
    fn copy_token_of_target_creates_copy() {
        let mut state = GameState::new_two_player(42);

        let target_id = create_object(
            &mut state,
            CardId(1),
            PlayerId(0),
            "Grizzly Bears".to_string(),
            Zone::Battlefield,
        );
        {
            let target = state.objects.get_mut(&target_id).unwrap();
            target.base_power = Some(2);
            target.base_toughness = Some(2);
            target.power = Some(2);
            target.toughness = Some(2);
        }

        let source_id = create_object(
            &mut state,
            CardId(2),
            PlayerId(0),
            "Copier".to_string(),
            Zone::Battlefield,
        );

        let mut events = Vec::new();
        let ability = ResolvedAbility::new(
            Effect::CopyTokenOf {
                target: TargetFilter::Any,
                enters_attacking: false,
                tapped: false,
            },
            vec![TargetRef::Object(target_id)],
            source_id,
            PlayerId(0),
        );

        resolve(&mut state, &ability, &mut events).unwrap();

        let token_id = ObjectId(state.next_object_id - 1);
        let token = state.objects.get(&token_id).unwrap();
        assert_eq!(token.name, "Grizzly Bears");
        assert_eq!(token.power, Some(2));
        assert_eq!(token.toughness, Some(2));
        assert!(token.is_token);
    }

    #[test]
    fn copy_token_enters_tapped_and_attacking() {
        let mut state = GameState::new_two_player(42);

        // Set up combat
        state.combat = Some(crate::game::combat::CombatState::default());

        let source_id = create_object(
            &mut state,
            CardId(1),
            PlayerId(0),
            "Attacker".to_string(),
            Zone::Battlefield,
        );
        {
            let source = state.objects.get_mut(&source_id).unwrap();
            source.base_power = Some(2);
            source.base_toughness = Some(2);
            source.power = Some(2);
            source.toughness = Some(2);
        }

        let mut events = Vec::new();
        let ability = ResolvedAbility::new(
            Effect::CopyTokenOf {
                target: TargetFilter::Any,
                enters_attacking: true,
                tapped: true,
            },
            vec![TargetRef::Object(source_id)],
            source_id,
            PlayerId(0),
        );

        resolve(&mut state, &ability, &mut events).unwrap();

        let token_id = ObjectId(state.next_object_id - 1);
        let token = state.objects.get(&token_id).unwrap();

        // CR 508.4: Token enters tapped and attacking
        assert!(token.tapped);
        let combat = state.combat.as_ref().unwrap();
        assert!(combat.attackers.iter().any(|a| a.object_id == token_id));
    }
}
