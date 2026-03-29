use crate::game::quantity::resolve_quantity;
use crate::game::speed::{increase_speed, set_speed};
use crate::types::ability::{Effect, EffectError, PlayerFilter, ResolvedAbility};
use crate::types::events::GameEvent;
use crate::types::game_state::GameState;
use crate::types::player::PlayerId;

fn players_for_filter(
    state: &GameState,
    filter: &PlayerFilter,
    controller: PlayerId,
) -> Vec<PlayerId> {
    match filter {
        PlayerFilter::Controller => vec![controller],
        PlayerFilter::Opponent => state
            .players
            .iter()
            .filter(|player| !player.is_eliminated && player.id != controller)
            .map(|player| player.id)
            .collect(),
        PlayerFilter::OpponentLostLife => state
            .players
            .iter()
            .filter(|player| !player.is_eliminated)
            .filter(|player| player.id != controller && player.life_lost_this_turn > 0)
            .map(|player| player.id)
            .collect(),
        PlayerFilter::OpponentGainedLife => state
            .players
            .iter()
            .filter(|player| !player.is_eliminated)
            .filter(|player| player.id != controller && player.life_gained_this_turn > 0)
            .map(|player| player.id)
            .collect(),
        PlayerFilter::All => state
            .players
            .iter()
            .filter(|player| !player.is_eliminated)
            .map(|player| player.id)
            .collect(),
        PlayerFilter::HighestSpeed => {
            let highest_speed = state
                .players
                .iter()
                .filter(|player| !player.is_eliminated)
                .map(|player| player.speed.unwrap_or(0))
                .max()
                .unwrap_or(0);
            state
                .players
                .iter()
                .filter(|player| !player.is_eliminated)
                .filter(|player| player.speed.unwrap_or(0) == highest_speed)
                .map(|player| player.id)
                .collect()
        }
    }
}

/// CR 702.179a: Effects that instruct players to start their engines set speed to 1
/// only if the player currently has no speed.
pub fn resolve_start(
    state: &mut GameState,
    ability: &ResolvedAbility,
    events: &mut Vec<GameEvent>,
) -> Result<(), EffectError> {
    let Effect::StartYourEngines { player_scope } = &ability.effect else {
        return Err(EffectError::InvalidParam(
            "expected StartYourEngines".to_string(),
        ));
    };

    for player_id in players_for_filter(state, player_scope, ability.controller) {
        let has_no_speed = state
            .players
            .iter()
            .find(|player| player.id == player_id)
            .is_some_and(|player| player.speed.is_none());
        if has_no_speed {
            set_speed(state, player_id, Some(1), events);
        }
    }

    Ok(())
}

/// CR 702.179c-d: Increase speed by the resolved amount for each selected player.
pub fn resolve_increase(
    state: &mut GameState,
    ability: &ResolvedAbility,
    events: &mut Vec<GameEvent>,
) -> Result<(), EffectError> {
    let Effect::IncreaseSpeed {
        player_scope,
        amount,
    } = &ability.effect
    else {
        return Err(EffectError::InvalidParam(
            "expected IncreaseSpeed".to_string(),
        ));
    };

    let amount = resolve_quantity(state, amount, ability.controller, ability.source_id);
    let amount = u8::try_from(amount.max(0)).unwrap_or(u8::MAX);
    if amount == 0 {
        return Ok(());
    }

    for player_id in players_for_filter(state, player_scope, ability.controller) {
        increase_speed(state, player_id, amount, events);
    }

    Ok(())
}
