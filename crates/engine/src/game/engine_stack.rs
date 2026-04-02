use crate::types::ability::{ResolvedAbility, TargetRef};
use crate::types::events::GameEvent;
use crate::types::game_state::{
    GameState, TargetSelectionConstraint, TargetSelectionSlot, WaitingFor,
};
use crate::types::player::PlayerId;

use super::ability_utils::{
    assign_selected_slots_in_chain, assign_targets_in_chain, choose_target,
    flatten_targets_in_chain, validate_selected_targets, TargetSelectionAdvance,
};
use super::effects;
use super::engine::{resume_pending_continuation_if_priority, EngineError};
use super::triggers::PendingTrigger;
use super::{casting, triggers};

fn finalize_trigger_target_selection(
    state: &mut GameState,
    trigger: PendingTrigger,
    ability: ResolvedAbility,
    events: &mut Vec<GameEvent>,
) {
    casting::emit_targeting_events(
        state,
        &flatten_targets_in_chain(&ability),
        trigger.source_id,
        trigger.controller,
        events,
    );

    let mut trigger = trigger;
    trigger.ability = ability;
    triggers::push_pending_trigger_to_stack(state, trigger, events);
    state.priority_passes.clear();
    state.priority_pass_count = 0;
}

pub(super) fn handle_trigger_target_selection_select_targets(
    state: &mut GameState,
    player: PlayerId,
    target_slots: &[TargetSelectionSlot],
    target_constraints: &[TargetSelectionConstraint],
    targets: Vec<TargetRef>,
    events: &mut Vec<GameEvent>,
) -> Result<WaitingFor, EngineError> {
    validate_selected_targets(target_slots, &targets, target_constraints)?;

    let trigger = state
        .pending_trigger
        .take()
        .ok_or_else(|| EngineError::InvalidAction("No pending trigger".to_string()))?;
    let mut ability = trigger.ability.clone();
    assign_targets_in_chain(&mut ability, &targets)?;

    finalize_trigger_target_selection(state, trigger, ability, events);
    Ok(WaitingFor::Priority { player })
}

pub(super) fn handle_trigger_target_selection_choose_target(
    state: &mut GameState,
    waiting_for: WaitingFor,
    target: Option<TargetRef>,
    events: &mut Vec<GameEvent>,
) -> Result<WaitingFor, EngineError> {
    let (player, target_slots, target_constraints, selection, source_id, description) =
        match waiting_for {
            WaitingFor::TriggerTargetSelection {
                player,
                target_slots,
                target_constraints,
                selection,
                source_id,
                description,
            } => (
                player,
                target_slots,
                target_constraints,
                selection,
                source_id,
                description,
            ),
            _ => {
                return Err(EngineError::InvalidAction(
                    "Not waiting for trigger target selection".to_string(),
                ));
            }
        };

    let Some(_pending_trigger) = state.pending_trigger.as_ref() else {
        return Err(EngineError::InvalidAction("No pending trigger".to_string()));
    };

    match choose_target(&target_slots, &target_constraints, &selection, target)? {
        TargetSelectionAdvance::InProgress(selection) => Ok(WaitingFor::TriggerTargetSelection {
            player,
            target_slots,
            target_constraints,
            selection,
            source_id,
            description,
        }),
        TargetSelectionAdvance::Complete(selected_slots) => {
            let trigger = state
                .pending_trigger
                .take()
                .ok_or_else(|| EngineError::InvalidAction("No pending trigger".to_string()))?;
            let mut ability = trigger.ability.clone();
            assign_selected_slots_in_chain(&mut ability, &selected_slots)?;

            finalize_trigger_target_selection(state, trigger, ability, events);
            Ok(WaitingFor::Priority { player })
        }
    }
}

pub(super) fn handle_multi_target_selection(
    state: &mut GameState,
    waiting_for: WaitingFor,
    selected: &[crate::types::identifiers::ObjectId],
    events: &mut Vec<GameEvent>,
) -> Result<WaitingFor, EngineError> {
    let (player, legal_targets, min_targets, max_targets, pending_ability) = match waiting_for {
        WaitingFor::MultiTargetSelection {
            player,
            legal_targets,
            min_targets,
            max_targets,
            pending_ability,
        } => (
            player,
            legal_targets,
            min_targets,
            max_targets,
            pending_ability.as_ref().clone(),
        ),
        _ => {
            return Err(EngineError::InvalidAction(
                "Not waiting for multi-target selection".to_string(),
            ));
        }
    };

    if selected.len() < min_targets || selected.len() > max_targets {
        return Err(EngineError::InvalidAction(format!(
            "Must select between {} and {} targets, got {}",
            min_targets,
            max_targets,
            selected.len()
        )));
    }

    for id in selected {
        if !legal_targets.contains(id) {
            return Err(EngineError::InvalidAction(
                "Selected target not in legal set".to_string(),
            ));
        }
    }

    let mut ability = pending_ability;
    ability.targets = selected.iter().map(|&id| TargetRef::Object(id)).collect();

    state.waiting_for = WaitingFor::Priority { player };
    state.priority_player = player;
    let _ = effects::resolve_ability_chain(state, &ability, events, 0);
    resume_pending_continuation_if_priority(state, events)?;

    Ok(state.waiting_for.clone())
}
