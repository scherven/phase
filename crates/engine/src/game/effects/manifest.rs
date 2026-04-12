use crate::game::quantity::resolve_quantity_with_targets;
use crate::types::ability::{Effect, EffectError, EffectKind, ResolvedAbility};
use crate::types::events::GameEvent;
use crate::types::game_state::GameState;

/// CR 701.40a: Manifest — turn the top card of the controller's library face down,
/// making it a 2/2 creature with no text, no name, no subtypes, and no mana cost,
/// and put it onto the battlefield.
///
/// CR 701.40e: If manifesting multiple cards, manifest them one at a time.
pub fn resolve(
    state: &mut GameState,
    ability: &ResolvedAbility,
    events: &mut Vec<GameEvent>,
) -> Result<(), EffectError> {
    let count = match &ability.effect {
        Effect::Manifest { count } => {
            resolve_quantity_with_targets(state, count, ability).max(0) as usize
        }
        _ => return Err(EffectError::MissingParam("count".to_string())),
    };

    let player = ability.controller;

    // CR 701.40e: Manifest cards one at a time
    for _ in 0..count {
        let has_cards = state
            .players
            .iter()
            .find(|p| p.id == player)
            .map(|p| !p.library.is_empty())
            .unwrap_or(false);

        if !has_cards {
            break;
        }

        // CR 701.40a: Manifest the top card using the shared morph infrastructure
        crate::game::morph::manifest(state, player, events)
            .map_err(|e| EffectError::MissingParam(format!("{e}")))?;
    }

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
    use crate::types::ability::QuantityExpr;
    use crate::types::identifiers::{CardId, ObjectId};
    use crate::types::player::PlayerId;
    use crate::types::zones::Zone;

    fn make_manifest_ability(count: i32) -> ResolvedAbility {
        ResolvedAbility::new(
            Effect::Manifest {
                count: QuantityExpr::Fixed { value: count },
            },
            vec![],
            ObjectId(100),
            PlayerId(0),
        )
    }

    #[test]
    fn manifest_single_card() {
        let mut state = GameState::new_two_player(42);
        let player = PlayerId(0);
        let id = create_object(
            &mut state,
            CardId(1),
            player,
            "Test Card".to_string(),
            Zone::Library,
        );

        let ability = make_manifest_ability(1);
        let mut events = Vec::new();
        resolve(&mut state, &ability, &mut events).unwrap();

        let obj = &state.objects[&id];
        assert!(obj.face_down);
        assert_eq!(obj.zone, Zone::Battlefield);
        assert_eq!(obj.power, Some(2));
        assert_eq!(obj.toughness, Some(2));
    }

    #[test]
    fn manifest_multiple_cards() {
        let mut state = GameState::new_two_player(42);
        let player = PlayerId(0);
        let id1 = create_object(
            &mut state,
            CardId(1),
            player,
            "Card A".to_string(),
            Zone::Library,
        );
        let id2 = create_object(
            &mut state,
            CardId(2),
            player,
            "Card B".to_string(),
            Zone::Library,
        );

        let ability = make_manifest_ability(2);
        let mut events = Vec::new();
        resolve(&mut state, &ability, &mut events).unwrap();

        // Both should be manifested face-down on battlefield
        for id in [id1, id2] {
            let obj = &state.objects[&id];
            assert!(obj.face_down, "Card {id:?} should be face down");
            assert_eq!(obj.zone, Zone::Battlefield);
            assert_eq!(obj.power, Some(2));
            assert_eq!(obj.toughness, Some(2));
        }
    }

    #[test]
    fn manifest_empty_library_does_nothing() {
        let mut state = GameState::new_two_player(42);
        assert!(state.players[0].library.is_empty());

        let ability = make_manifest_ability(1);
        let mut events = Vec::new();
        let result = resolve(&mut state, &ability, &mut events);
        assert!(result.is_ok());
    }

    #[test]
    fn manifest_more_than_library_manifests_available() {
        let mut state = GameState::new_two_player(42);
        let player = PlayerId(0);
        create_object(
            &mut state,
            CardId(1),
            player,
            "Only Card".to_string(),
            Zone::Library,
        );

        // Try to manifest 3, but only 1 card in library
        let ability = make_manifest_ability(3);
        let mut events = Vec::new();
        resolve(&mut state, &ability, &mut events).unwrap();

        // Should have manifested the one available card
        let battlefield_count = state
            .objects
            .values()
            .filter(|o| o.zone == Zone::Battlefield && o.face_down)
            .count();
        assert_eq!(battlefield_count, 1);
    }
}
