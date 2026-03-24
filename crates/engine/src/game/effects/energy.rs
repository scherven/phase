use crate::types::ability::{Effect, EffectError, EffectKind, ResolvedAbility};
use crate::types::events::GameEvent;
use crate::types::game_state::GameState;

/// CR 122.1: Gain energy counters. Increments the controller's energy pool.
pub fn resolve_gain(
    state: &mut GameState,
    ability: &ResolvedAbility,
    events: &mut Vec<GameEvent>,
) -> Result<(), EffectError> {
    let amount = match &ability.effect {
        Effect::GainEnergy { amount } => *amount,
        _ => return Err(EffectError::MissingParam("amount".to_string())),
    };

    // CR 122.1: Energy counters are a kind of counter that a player may have.
    let player = &mut state.players[ability.controller.0 as usize];
    player.energy += amount;

    // CR 122.1 + CR 107.14: Energy counters are counters placed on a player.
    events.push(GameEvent::EnergyChanged {
        player: ability.controller,
        delta: amount as i32,
    });
    events.push(GameEvent::EffectResolved {
        kind: EffectKind::GainEnergy,
        source_id: ability.source_id,
    });

    Ok(())
}
