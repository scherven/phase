use crate::game::quantity;
use crate::types::ability::{
    Effect, EffectError, EffectKind, ResolvedAbility, TargetFilter, TargetRef,
};
use crate::types::events::GameEvent;
use crate::types::game_state::GameState;

/// CR 122.1: Give player counters of a named type.
/// Poison counters dispatch to the dedicated field (CR 104.3d SBA).
/// All other counter types use the generic player_counters map.
pub fn resolve(
    state: &mut GameState,
    ability: &ResolvedAbility,
    events: &mut Vec<GameEvent>,
) -> Result<(), EffectError> {
    let (counter_kind, count, target) = match &ability.effect {
        Effect::GivePlayerCounter {
            counter_kind,
            count,
            target,
        } => (counter_kind, count, target),
        _ => {
            return Err(EffectError::MissingParam(
                "expected GivePlayerCounter".into(),
            ))
        }
    };

    // CR 122.1: Resolve the quantity to a concrete count.
    let raw = quantity::resolve_quantity_with_targets(state, count, ability);
    let amount = raw.max(0) as u32;
    if amount == 0 {
        return Ok(());
    }

    // Resolve target player(s)
    let players = match target {
        TargetFilter::Controller | TargetFilter::SelfRef => vec![ability.controller],
        _ => {
            // Targeted: resolve from ability.targets
            let targeted: Vec<_> = ability
                .targets
                .iter()
                .filter_map(|t| match t {
                    TargetRef::Player(pid) => Some(*pid),
                    _ => None,
                })
                .collect();
            if targeted.is_empty() {
                // No valid targets — do nothing (fizzle already handled by stack.rs)
                return Ok(());
            } else {
                targeted
            }
        }
    };

    for player_id in &players {
        let player = &mut state.players[player_id.0 as usize];
        player.add_player_counters(counter_kind, amount);

        // CR 122.1: Emit event for counter change.
        events.push(GameEvent::PlayerCounterChanged {
            player: *player_id,
            counter_kind: counter_kind.clone(),
            delta: amount as i32,
        });
    }

    events.push(GameEvent::EffectResolved {
        kind: EffectKind::GivePlayerCounter,
        source_id: ability.source_id,
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ability::{AbilityKind, QuantityExpr, SpellContext, TargetFilter};
    use crate::types::identifiers::ObjectId;
    use crate::types::player::{PlayerCounterKind, PlayerId};

    fn make_ability(
        counter_kind: PlayerCounterKind,
        count: QuantityExpr,
        target: TargetFilter,
        controller: PlayerId,
    ) -> ResolvedAbility {
        ResolvedAbility {
            effect: Effect::GivePlayerCounter {
                counter_kind,
                count,
                target,
            },
            controller,
            source_id: ObjectId(1),
            targets: vec![],
            kind: AbilityKind::Spell,
            sub_ability: None,
            else_ability: None,
            duration: None,
            condition: None,
            context: SpellContext::default(),
            optional_targeting: false,
            optional: false,
            optional_for: None,
            multi_target: None,
            description: None,
            player_scope: None,
            chosen_x: None,
            repeat_for: None,
            forward_result: false,
            unless_pay: None,
            distribution: None,
        }
    }

    #[test]
    fn poison_counter_uses_dedicated_field() {
        let mut state = GameState::default();
        let mut events = Vec::new();
        let ability = make_ability(
            PlayerCounterKind::Poison,
            QuantityExpr::Fixed { value: 1 },
            TargetFilter::Controller,
            PlayerId(0),
        );

        resolve(&mut state, &ability, &mut events).unwrap();

        assert_eq!(state.players[0].poison_counters, 1);
        // Should NOT be in the generic map
        assert_eq!(
            state.players[0]
                .player_counters
                .get(&PlayerCounterKind::Poison),
            None
        );
    }

    #[test]
    fn experience_counter_uses_generic_map() {
        let mut state = GameState::default();
        let mut events = Vec::new();
        let ability = make_ability(
            PlayerCounterKind::Experience,
            QuantityExpr::Fixed { value: 2 },
            TargetFilter::Controller,
            PlayerId(0),
        );

        resolve(&mut state, &ability, &mut events).unwrap();

        assert_eq!(
            state.players[0].player_counter(&PlayerCounterKind::Experience),
            2
        );
    }

    #[test]
    fn counter_accumulates() {
        let mut state = GameState::default();
        let mut events = Vec::new();

        let ability = make_ability(
            PlayerCounterKind::Rad,
            QuantityExpr::Fixed { value: 3 },
            TargetFilter::Controller,
            PlayerId(0),
        );

        resolve(&mut state, &ability, &mut events).unwrap();
        resolve(&mut state, &ability, &mut events).unwrap();

        assert_eq!(state.players[0].player_counter(&PlayerCounterKind::Rad), 6);
    }

    #[test]
    fn targeted_player_counter() {
        let mut state = GameState::default();
        let mut events = Vec::new();
        let mut ability = make_ability(
            PlayerCounterKind::Poison,
            QuantityExpr::Fixed { value: 1 },
            TargetFilter::Any,
            PlayerId(0),
        );
        ability.targets = vec![TargetRef::Player(PlayerId(1))];

        resolve(&mut state, &ability, &mut events).unwrap();

        assert_eq!(state.players[0].poison_counters, 0);
        assert_eq!(state.players[1].poison_counters, 1);
    }

    #[test]
    fn emits_counter_changed_event() {
        let mut state = GameState::default();
        let mut events = Vec::new();
        let ability = make_ability(
            PlayerCounterKind::Ticket,
            QuantityExpr::Fixed { value: 1 },
            TargetFilter::Controller,
            PlayerId(0),
        );

        resolve(&mut state, &ability, &mut events).unwrap();

        assert!(events.iter().any(|e| matches!(
            e,
            GameEvent::PlayerCounterChanged {
                counter_kind,
                delta: 1,
                ..
            } if *counter_kind == PlayerCounterKind::Ticket
        )));
    }
}
