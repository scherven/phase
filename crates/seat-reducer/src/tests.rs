use engine::game::deck_loading::PlayerDeckPayload;
use engine::types::format::FormatConfig;
use phase_ai::config::Platform;

use crate::types::*;

struct NoOpResolver;
impl DeckResolver for NoOpResolver {
    fn resolve(&self, _choice: &DeckChoice) -> Result<PlayerDeckPayload, String> {
        Err("no-op".to_string())
    }
}

fn ctx() -> ReducerCtx<'static> {
    static RESOLVER: NoOpResolver = NoOpResolver;
    ReducerCtx {
        platform: Platform::Native,
        deck_resolver: &RESOLVER,
    }
}

fn two_player_full() -> SeatState {
    SeatState {
        seats: vec![
            SeatKind::HostHuman,
            SeatKind::Ai {
                difficulty: phase_ai::config::AiDifficulty::Medium,
                deck: DeckChoice::Random,
            },
        ],
        tokens: vec!["host-token".to_string(), String::new()],
        format: FormatConfig::standard(),
        game_started: false,
    }
}

fn two_player_waiting() -> SeatState {
    SeatState {
        seats: vec![SeatKind::HostHuman, SeatKind::WaitingHuman],
        tokens: vec!["host-token".to_string(), String::new()],
        format: FormatConfig::standard(),
        game_started: false,
    }
}

#[test]
fn start_on_full_room_succeeds() {
    let mut state = two_player_full();
    let delta = crate::apply(&mut state, SeatMutation::Start, &ctx()).unwrap();
    assert!(delta.now_started);
    assert!(state.game_started);
}

#[test]
fn start_on_not_full_room_fails() {
    let mut state = two_player_waiting();
    let err = crate::apply(&mut state, SeatMutation::Start, &ctx()).unwrap_err();
    assert_eq!(err, SeatError::NotFull);
    assert!(!state.game_started);
}

#[test]
fn start_after_game_started_fails() {
    let mut state = two_player_full();
    state.game_started = true;
    let err = crate::apply(&mut state, SeatMutation::Start, &ctx()).unwrap_err();
    assert_eq!(err, SeatError::GameStarted);
}

#[test]
fn is_full_requires_all_claimed() {
    let full = two_player_full();
    assert!(full.is_full());

    let waiting = two_player_waiting();
    assert!(!waiting.is_full());
}

#[test]
fn is_full_with_joined_human() {
    let state = SeatState {
        seats: vec![SeatKind::HostHuman, SeatKind::JoinedHuman],
        tokens: vec!["host".to_string(), "guest".to_string()],
        format: FormatConfig::standard(),
        game_started: false,
    };
    assert!(state.is_full());
}

#[test]
fn to_view_strips_tokens() {
    let state = two_player_full();
    let view = state.to_view();
    assert_eq!(view.seats, state.seats);
    assert!(view.is_full);
    assert!(!view.game_started);
    // SeatView has no tokens field — the type itself enforces the invariant
}

#[test]
fn is_pregame_reflects_game_started() {
    let mut state = two_player_full();
    assert!(state.is_pregame());
    state.game_started = true;
    assert!(!state.is_pregame());
}

#[test]
fn set_kind_returns_placeholder_error() {
    let mut state = two_player_waiting();
    let err = crate::apply(
        &mut state,
        SeatMutation::SetKind {
            seat_index: 1,
            kind: SeatKind::Ai {
                difficulty: phase_ai::config::AiDifficulty::Hard,
                deck: DeckChoice::Random,
            },
        },
        &ctx(),
    )
    .unwrap_err();
    assert_eq!(err, SeatError::InvalidTransition);
}

#[test]
fn remove_returns_placeholder_error() {
    let mut state = two_player_waiting();
    let err = crate::apply(&mut state, SeatMutation::Remove { seat_index: 1 }, &ctx()).unwrap_err();
    assert_eq!(err, SeatError::InvalidTransition);
}
