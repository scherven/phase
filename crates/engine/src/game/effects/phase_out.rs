use crate::types::ability::{EffectError, EffectKind, ResolvedAbility};
use crate::types::events::GameEvent;
use crate::types::game_state::GameState;

/// CR 702.26a: Phase out — the target permanent is treated as though it doesn't
/// exist until its controller's next untap step.
///
/// Stub: emits EffectResolved event. Full game logic is a follow-up.
// CR 702.26b: A phased-out permanent is treated as though it does not exist.
// CR 702.26d: The phasing event doesn't actually cause a zone change.
// TODO(CR 702.26): Phase out is unimplemented — should remove permanent from game temporarily, preserving all state.
pub fn resolve(
    state: &mut GameState,
    ability: &ResolvedAbility,
    events: &mut Vec<GameEvent>,
) -> Result<(), EffectError> {
    let _ = state;
    events.push(GameEvent::EffectResolved {
        kind: EffectKind::PhaseOut,
        source_id: ability.source_id,
    });
    Ok(())
}
