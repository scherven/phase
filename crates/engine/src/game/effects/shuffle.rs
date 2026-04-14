use rand::seq::SliceRandom;

use crate::types::ability::{EffectError, EffectKind, ResolvedAbility, TargetRef};
use crate::types::events::GameEvent;
use crate::types::game_state::GameState;

/// CR 701.24a: Shuffle — randomize the cards in a library.
pub fn resolve(
    state: &mut GameState,
    ability: &ResolvedAbility,
    events: &mut Vec<GameEvent>,
) -> Result<(), EffectError> {
    let target_player = ability
        .targets
        .iter()
        .find_map(|t| {
            if let TargetRef::Player(pid) = t {
                Some(*pid)
            } else {
                None
            }
        })
        .unwrap_or(ability.controller);

    // CR 701.24: "Can't shuffle" suppresses library shuffling. Per CR 701.24d,
    // if a player would shuffle their library and can't, they don't shuffle.
    // The effect itself still resolves (EffectResolved fires below).
    let suppressed =
        crate::game::static_abilities::player_has_static_other(state, target_player, "CantShuffle");

    if !suppressed {
        let player = state
            .players
            .iter_mut()
            .find(|p| p.id == target_player)
            .ok_or(EffectError::PlayerNotFound)?;

        // CR 701.24a: Randomize cards so that no player knows their order.
        player.library.shuffle(&mut state.rng);
    }

    events.push(GameEvent::EffectResolved {
        kind: EffectKind::Shuffle,
        source_id: ability.source_id,
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::zones::create_object;
    use crate::types::ability::{Effect, TargetFilter};
    use crate::types::identifiers::{CardId, ObjectId};
    use crate::types::player::PlayerId;
    use crate::types::zones::Zone;

    fn make_shuffle_ability(targets: Vec<TargetRef>) -> ResolvedAbility {
        ResolvedAbility::new(
            Effect::Shuffle {
                target: TargetFilter::Controller,
            },
            targets,
            ObjectId(100),
            PlayerId(0),
        )
    }

    #[test]
    fn shuffle_emits_effect_resolved() {
        let mut state = GameState::new_two_player(42);
        for i in 0..5 {
            create_object(
                &mut state,
                CardId(i + 1),
                PlayerId(0),
                format!("Card {}", i),
                Zone::Library,
            );
        }

        let ability = make_shuffle_ability(vec![]);
        let mut events = Vec::new();

        resolve(&mut state, &ability, &mut events).unwrap();

        assert!(events.iter().any(|e| matches!(
            e,
            GameEvent::EffectResolved {
                kind: EffectKind::Shuffle,
                ..
            }
        )));
    }

    #[test]
    fn shuffle_preserves_library_size() {
        let mut state = GameState::new_two_player(42);
        for i in 0..10 {
            create_object(
                &mut state,
                CardId(i + 1),
                PlayerId(0),
                format!("Card {}", i),
                Zone::Library,
            );
        }
        let original_ids: Vec<_> = state.players[0].library.clone();

        let ability = make_shuffle_ability(vec![]);
        let mut events = Vec::new();

        resolve(&mut state, &ability, &mut events).unwrap();

        let shuffled_ids = &state.players[0].library;
        assert_eq!(shuffled_ids.len(), original_ids.len());
        let mut sorted_original = original_ids.clone();
        let mut sorted_shuffled = shuffled_ids.clone();
        sorted_original.sort_by_key(|id| id.0);
        sorted_shuffled.sort_by_key(|id| id.0);
        assert_eq!(sorted_original, sorted_shuffled);
    }

    #[test]
    fn cant_shuffle_preserves_library_order() {
        // CR 701.24: A player under "Can't shuffle" doesn't shuffle their library.
        use crate::types::ability::StaticDefinition;
        use crate::types::statics::StaticMode;

        let mut state = GameState::new_two_player(42);
        for i in 0..20 {
            create_object(
                &mut state,
                CardId(i + 1),
                PlayerId(0),
                format!("Card {}", i),
                Zone::Library,
            );
        }
        let before = state.players[0].library.clone();

        // Install CantShuffle static controlled by the affected player.
        let source = create_object(
            &mut state,
            CardId(999),
            PlayerId(0),
            "Aven Mindcensor".to_string(),
            Zone::Battlefield,
        );
        use crate::types::ability::{ControllerRef, TypedFilter};
        state
            .objects
            .get_mut(&source)
            .unwrap()
            .static_definitions
            .push(
                StaticDefinition::new(StaticMode::Other("CantShuffle".to_string())).affected(
                    TargetFilter::Typed(TypedFilter::default().controller(ControllerRef::You)),
                ),
            );

        let ability = make_shuffle_ability(vec![]);
        let mut events = Vec::new();
        resolve(&mut state, &ability, &mut events).unwrap();

        assert_eq!(
            state.players[0].library, before,
            "library order must be preserved under CantShuffle"
        );
        // EffectResolved still fires.
        assert!(events.iter().any(|e| matches!(
            e,
            GameEvent::EffectResolved {
                kind: EffectKind::Shuffle,
                ..
            }
        )));
    }

    #[test]
    fn shuffle_targets_specified_player() {
        let mut state = GameState::new_two_player(42);
        for i in 0..5 {
            create_object(
                &mut state,
                CardId(i + 1),
                PlayerId(1),
                format!("Card {}", i),
                Zone::Library,
            );
        }
        let p1_lib_before = state.players[1].library.clone();
        let p0_lib_before = state.players[0].library.clone();

        let ability = make_shuffle_ability(vec![TargetRef::Player(PlayerId(1))]);
        let mut events = Vec::new();

        resolve(&mut state, &ability, &mut events).unwrap();

        assert_eq!(state.players[0].library, p0_lib_before);
        assert_eq!(state.players[1].library.len(), p1_lib_before.len());
    }
}
