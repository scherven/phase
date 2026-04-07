use crate::types::events::GameEvent;
use crate::types::game_state::{GameState, WaitingFor};

use super::engine::{begin_pending_trigger_target_selection, check_exile_returns, EngineError};
use super::match_flow;
use super::sba;
use super::triggers;

pub(super) fn run_post_action_pipeline(
    state: &mut GameState,
    events: &mut Vec<GameEvent>,
    default_wf: &WaitingFor,
    skip_trigger_scan: bool,
) -> Result<WaitingFor, EngineError> {
    sba::check_state_based_actions(state, events);

    if !matches!(state.waiting_for, WaitingFor::Priority { .. }) {
        if matches!(state.waiting_for, WaitingFor::GameOver { .. }) {
            match_flow::handle_game_over_transition(state);
        }
        return Ok(state.waiting_for.clone());
    }

    check_exile_returns(state, events);

    let delayed_events = triggers::check_delayed_triggers(state, events);
    events.extend(delayed_events);

    let stack_before = state.stack.len();

    if !skip_trigger_scan {
        let filtered_events: Vec<_> = events
            .iter()
            .filter(|event| !matches!(event, GameEvent::PhaseChanged { .. }))
            .cloned()
            .collect();
        triggers::process_triggers(state, &filtered_events);
    }

    // CR 603.8: Check state triggers after event-based triggers.
    // State triggers fire when a condition is true, checked whenever a player
    // would receive priority.
    triggers::check_state_triggers(state);

    if let Some(waiting_for) = begin_pending_trigger_target_selection(state)? {
        state.waiting_for = waiting_for.clone();
        return Ok(waiting_for);
    }

    if state.stack.len() > stack_before {
        return Ok(WaitingFor::Priority {
            player: state.active_player,
        });
    }

    if state.layers_dirty {
        super::layers::evaluate_layers(state);
    }

    Ok(default_wf.clone())
}
