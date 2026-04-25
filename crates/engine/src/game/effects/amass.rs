use crate::game::effects::counters::add_counter_with_replacement;
use crate::game::quantity::resolve_quantity_with_targets;
use crate::game::zones;
use crate::types::ability::{Effect, EffectError, EffectKind, ResolvedAbility};
use crate::types::card_type::{CardType, CoreType};
use crate::types::counter::CounterType;
use crate::types::events::GameEvent;
use crate::types::game_state::GameState;
use crate::types::identifiers::{CardId, ObjectId};
use crate::types::mana::ManaColor;
use crate::types::zones::Zone;

/// CR 701.47a: Amass [subtype] N.
///
/// If you don't control an Army creature, create a 0/0 black [subtype] Army
/// creature token. Choose an Army creature you control. Put N +1/+1 counters
/// on that creature. If it isn't a [subtype], it becomes a [subtype] in
/// addition to its other types.
pub fn resolve(
    state: &mut GameState,
    ability: &ResolvedAbility,
    events: &mut Vec<GameEvent>,
) -> Result<(), EffectError> {
    let (subtype, count_expr) = match &ability.effect {
        Effect::Amass { subtype, count } => (subtype.clone(), count.clone()),
        _ => return Ok(()),
    };

    let controller = ability.controller;
    let n = resolve_quantity_with_targets(state, &count_expr, ability).max(0) as u32;

    // CR 701.47a: Find an existing Army creature on the controller's battlefield.
    let army_id = find_army(state, controller);

    let target_id = if let Some(id) = army_id {
        id
    } else {
        // CR 701.47a: Create a 0/0 black [subtype] Army creature token.
        create_army_token(state, controller, &subtype)
    };

    // CR 701.47a: Put N +1/+1 counters on the chosen Army.
    if n > 0 {
        add_counter_with_replacement(state, target_id, CounterType::Plus1Plus1, n, events);
    }

    // CR 701.47a: If it isn't a [subtype], it becomes a [subtype] in addition to its other types.
    if let Some(obj) = state.objects.get_mut(&target_id) {
        if !obj
            .card_types
            .subtypes
            .iter()
            .any(|s| s.eq_ignore_ascii_case(&subtype))
        {
            obj.card_types.subtypes.push(subtype.clone());
            obj.base_card_types.subtypes.push(subtype);
        }
    }

    events.push(GameEvent::EffectResolved {
        kind: EffectKind::Amass,
        source_id: ability.source_id,
    });

    Ok(())
}

/// Find the first Army creature controlled by `controller` on the battlefield.
/// CR 701.47a: If multiple Armies exist, auto-select deterministically by ObjectId.
fn find_army(state: &GameState, controller: crate::types::player::PlayerId) -> Option<ObjectId> {
    state
        .battlefield
        .iter()
        .filter_map(|&id| state.objects.get(&id).map(|obj| (id, obj)))
        .filter(|(_, obj)| {
            obj.controller == controller
                && obj.card_types.core_types.contains(&CoreType::Creature)
                && obj.card_types.subtypes.iter().any(|s| s == "Army")
        })
        .map(|(id, _)| id)
        .min_by_key(|id| id.0) // deterministic: lowest ObjectId
}

/// Create a 0/0 black [subtype] Army creature token on the battlefield.
fn create_army_token(
    state: &mut GameState,
    controller: crate::types::player::PlayerId,
    subtype: &str,
) -> ObjectId {
    let name = format!("{subtype} Army");
    let obj_id = zones::create_object(state, CardId(0), controller, name, Zone::Battlefield);

    if let Some(obj) = state.objects.get_mut(&obj_id) {
        obj.is_token = true;
        obj.power = Some(0);
        obj.toughness = Some(0);
        obj.base_power = Some(0);
        obj.base_toughness = Some(0);
        obj.card_types = CardType {
            supertypes: vec![],
            core_types: vec![CoreType::Creature],
            subtypes: vec!["Army".to_string(), subtype.to_string()],
        };
        obj.base_card_types = obj.card_types.clone();
        obj.color = vec![ManaColor::Black];
        obj.base_color = vec![ManaColor::Black];
        // CR 400.7 + CR 302.6: Single authority for ETB state — sets
        // `entered_battlefield_turn`, `summoning_sick`, and clears transient
        // fields (tapped, damage_marked, etc.) in one place.
        obj.reset_for_battlefield_entry(state.turn_number);
    }

    obj_id
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ability::QuantityExpr;
    use crate::types::identifiers::ObjectId;
    use crate::types::player::PlayerId;

    fn make_amass_ability(subtype: &str, count: QuantityExpr) -> ResolvedAbility {
        ResolvedAbility::new(
            Effect::Amass {
                subtype: subtype.to_string(),
                count,
            },
            vec![],
            ObjectId(100),
            PlayerId(0),
        )
    }

    #[test]
    fn amass_creates_army_token_with_counters() {
        let mut state = GameState::new_two_player(42);
        let mut events = Vec::new();
        let ability = make_amass_ability("Zombie", QuantityExpr::Fixed { value: 2 });

        resolve(&mut state, &ability, &mut events).unwrap();

        // Should have created one creature on the battlefield
        let armies: Vec<_> = state
            .battlefield
            .iter()
            .filter_map(|id| state.objects.get(id))
            .filter(|obj| obj.card_types.subtypes.iter().any(|s| s == "Army"))
            .collect();
        assert_eq!(armies.len(), 1);
        let army = armies[0];
        assert!(army.is_token);
        assert_eq!(army.power, Some(0));
        assert_eq!(army.toughness, Some(0));
        assert!(army.card_types.subtypes.contains(&"Zombie".to_string()));
        assert!(army.card_types.subtypes.contains(&"Army".to_string()));
        assert_eq!(army.color, vec![ManaColor::Black]);
        // 2 +1/+1 counters
        assert_eq!(
            army.counters.get(&CounterType::Plus1Plus1).copied(),
            Some(2)
        );
    }

    #[test]
    fn amass_reuses_existing_army_and_adds_counters() {
        let mut state = GameState::new_two_player(42);
        let mut events = Vec::new();

        // First amass creates the army
        let ability = make_amass_ability("Zombie", QuantityExpr::Fixed { value: 2 });
        resolve(&mut state, &ability, &mut events).unwrap();

        let army_count_before = state
            .battlefield
            .iter()
            .filter(|id| {
                state
                    .objects
                    .get(id)
                    .map(|o| o.card_types.subtypes.iter().any(|s| s == "Army"))
                    .unwrap_or(false)
            })
            .count();
        assert_eq!(army_count_before, 1);

        // Second amass should reuse the existing army
        events.clear();
        let ability2 = make_amass_ability("Zombie", QuantityExpr::Fixed { value: 3 });
        resolve(&mut state, &ability2, &mut events).unwrap();

        let armies: Vec<_> = state
            .battlefield
            .iter()
            .filter_map(|id| state.objects.get(id))
            .filter(|obj| obj.card_types.subtypes.iter().any(|s| s == "Army"))
            .collect();
        assert_eq!(armies.len(), 1); // Still just one army
        assert_eq!(
            armies[0].counters.get(&CounterType::Plus1Plus1).copied(),
            Some(5)
        ); // 2 + 3
    }

    #[test]
    fn amass_adds_missing_subtype_to_existing_army() {
        let mut state = GameState::new_two_player(42);
        let mut events = Vec::new();

        // Create with Zombie subtype
        let ability = make_amass_ability("Zombie", QuantityExpr::Fixed { value: 1 });
        resolve(&mut state, &ability, &mut events).unwrap();

        // Amass Orcs should add Orc subtype
        events.clear();
        let ability2 = make_amass_ability("Orc", QuantityExpr::Fixed { value: 1 });
        resolve(&mut state, &ability2, &mut events).unwrap();

        let army = state
            .battlefield
            .iter()
            .filter_map(|id| state.objects.get(id))
            .find(|obj| obj.card_types.subtypes.iter().any(|s| s == "Army"))
            .unwrap();
        assert!(army.card_types.subtypes.contains(&"Zombie".to_string()));
        assert!(army.card_types.subtypes.contains(&"Orc".to_string()));
    }
}
