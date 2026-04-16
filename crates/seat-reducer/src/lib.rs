pub mod types;

use types::{ReducerCtx, SeatDelta, SeatError, SeatKind, SeatMutation, SeatState};

/// Apply a seat mutation to the current state.
///
/// Phase 1 implements only the `Start` arm. `SetKind` and `Remove` return
/// `SeatError::InvalidTransition` as placeholders until Phase 2.
pub fn apply(
    state: &mut SeatState,
    mutation: SeatMutation,
    ctx: &ReducerCtx,
) -> Result<SeatDelta, SeatError> {
    if !state.is_pregame() {
        return Err(SeatError::GameStarted);
    }

    match mutation {
        SeatMutation::Start => apply_start(state, ctx),
        SeatMutation::SetKind { seat_index, kind } => apply_set_kind(state, seat_index, kind, ctx),
        SeatMutation::Remove { seat_index } => apply_remove(state, seat_index),
    }
}

fn apply_start(state: &mut SeatState, _ctx: &ReducerCtx) -> Result<SeatDelta, SeatError> {
    if !state.is_full() {
        return Err(SeatError::NotFull);
    }

    state.game_started = true;
    Ok(SeatDelta {
        now_started: true,
        ..SeatDelta::empty()
    })
}

fn apply_set_kind(
    _state: &mut SeatState,
    _seat_index: u8,
    _kind: SeatKind,
    _ctx: &ReducerCtx,
) -> Result<SeatDelta, SeatError> {
    // Phase 2: full transition matrix
    Err(SeatError::InvalidTransition)
}

fn apply_remove(_state: &mut SeatState, _seat_index: u8) -> Result<SeatDelta, SeatError> {
    // Phase 2: removal with renumbering
    Err(SeatError::InvalidTransition)
}

#[cfg(test)]
mod tests;
