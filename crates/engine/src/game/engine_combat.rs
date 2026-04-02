use crate::game::combat::{AttackTarget, DamageAssignment, DamageTarget, TrampleKind};
use crate::types::events::GameEvent;
use crate::types::game_state::{DamageSlot, GameState, WaitingFor};
use crate::types::identifiers::ObjectId;
use crate::types::phase::Phase;
use crate::types::player::PlayerId;
use crate::types::zones::Zone;

use super::engine::{begin_pending_trigger_target_selection, EngineError};
use super::priority;
use super::triggers;
use super::turns;

pub(super) fn handle_declare_attackers(
    state: &mut GameState,
    player: PlayerId,
    attacks: &[(ObjectId, AttackTarget)],
    events: &mut Vec<GameEvent>,
) -> Result<WaitingFor, EngineError> {
    if state.active_player != player {
        return Err(EngineError::WrongPlayer);
    }
    super::combat::declare_attackers(state, attacks, events).map_err(EngineError::InvalidAction)?;

    triggers::process_triggers(state, events);
    if let Some(waiting_for) = begin_pending_trigger_target_selection(state)? {
        return Ok(waiting_for);
    }

    if attacks.is_empty() {
        state.phase = Phase::EndCombat;
        events.push(GameEvent::PhaseChanged {
            phase: Phase::EndCombat,
        });
        state.combat = None;
        super::layers::prune_end_of_combat_effects(state);
        turns::advance_phase(state, events);
        Ok(turns::auto_advance(state, events))
    } else {
        priority::reset_priority(state);
        Ok(WaitingFor::Priority {
            player: state.active_player,
        })
    }
}

pub(super) fn handle_declare_blockers(
    state: &mut GameState,
    assignments: &[(ObjectId, ObjectId)],
    events: &mut Vec<GameEvent>,
) -> Result<WaitingFor, EngineError> {
    super::combat::declare_blockers(state, assignments, events)
        .map_err(EngineError::InvalidAction)?;

    triggers::process_triggers(state, events);
    if let Some(waiting_for) = begin_pending_trigger_target_selection(state)? {
        return Ok(waiting_for);
    }

    priority::reset_priority(state);
    Ok(WaitingFor::Priority {
        player: state.active_player,
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_assign_combat_damage(
    state: &mut GameState,
    player: PlayerId,
    attacker_id: ObjectId,
    total_damage: u32,
    blockers: &[DamageSlot],
    trample: Option<TrampleKind>,
    defending_player: PlayerId,
    attack_target: &AttackTarget,
    pw_loyalty: Option<u32>,
    pw_controller: Option<PlayerId>,
    assignments: &[(ObjectId, u32)],
    trample_damage: u32,
    controller_damage: u32,
    events: &mut Vec<GameEvent>,
) -> Result<WaitingFor, EngineError> {
    let assigned_total: u32 = assignments.iter().map(|(_, amount)| *amount).sum::<u32>()
        + trample_damage
        + controller_damage;
    if assigned_total != total_damage {
        return Err(EngineError::InvalidAction(format!(
            "Damage assignment total {} != attacker power {}",
            assigned_total, total_damage
        )));
    }

    let valid_blocker_ids: Vec<ObjectId> = blockers.iter().map(|slot| slot.blocker_id).collect();
    for (blocker_id, _) in assignments {
        if !valid_blocker_ids.contains(blocker_id) {
            return Err(EngineError::InvalidAction(format!(
                "{:?} is not a blocker of attacker {:?}",
                blocker_id, attacker_id
            )));
        }
    }

    if (trample_damage > 0 || controller_damage > 0) && trample.is_none() {
        return Err(EngineError::InvalidAction(
            "Cannot assign trample damage without trample".to_string(),
        ));
    }

    if controller_damage > 0 {
        let is_valid = trample == Some(TrampleKind::OverPlaneswalkers)
            && pw_controller.is_some()
            && matches!(attack_target, AttackTarget::Planeswalker(_));
        if !is_valid {
            return Err(EngineError::InvalidAction(
                "Controller damage only allowed with trample over planeswalkers attacking a planeswalker".to_string(),
            ));
        }

        let loyalty_threshold = pw_loyalty.unwrap_or(0);
        if trample_damage < loyalty_threshold {
            return Err(EngineError::InvalidAction(format!(
                "Trample over planeswalkers: must assign at least {} to PW before {} to controller",
                loyalty_threshold, controller_damage
            )));
        }
    }

    if trample.is_some() {
        for slot in blockers {
            let assigned = assignments
                .iter()
                .find(|(id, _)| *id == slot.blocker_id)
                .map(|(_, amount)| *amount)
                .unwrap_or(0);
            if assigned < slot.lethal_minimum {
                return Err(EngineError::InvalidAction(format!(
                    "Trample: blocker {:?} must receive at least {} lethal damage before excess to player",
                    slot.blocker_id, slot.lethal_minimum
                )));
            }
        }
    }

    if let Some(combat) = &mut state.combat {
        for (blocker_id, amount) in assignments {
            if *amount > 0 {
                combat.pending_damage.push((
                    attacker_id,
                    DamageAssignment {
                        target: DamageTarget::Object(*blocker_id),
                        amount: *amount,
                    },
                ));
            }
        }

        if trample_damage > 0 {
            let is_over_pw = trample == Some(TrampleKind::OverPlaneswalkers);
            let excess_target = match attack_target {
                AttackTarget::Player(player_id) => Some(DamageTarget::Player(*player_id)),
                AttackTarget::Planeswalker(pw_id) => match state.objects.get(pw_id) {
                    Some(obj) if obj.zone == Zone::Battlefield => {
                        Some(DamageTarget::Object(*pw_id))
                    }
                    _ if is_over_pw => Some(DamageTarget::Player(defending_player)),
                    _ => None,
                },
                AttackTarget::Battle(battle_id) => match state.objects.get(battle_id) {
                    Some(obj) if obj.zone == Zone::Battlefield => {
                        Some(DamageTarget::Object(*battle_id))
                    }
                    _ => None,
                },
            };
            if let Some(target) = excess_target {
                combat.pending_damage.push((
                    attacker_id,
                    DamageAssignment {
                        target,
                        amount: trample_damage,
                    },
                ));
            }
        }

        if controller_damage > 0 {
            if let Some(controller) = pw_controller {
                combat.pending_damage.push((
                    attacker_id,
                    DamageAssignment {
                        target: DamageTarget::Player(controller),
                        amount: controller_damage,
                    },
                ));
            }
        }

        combat.damage_step_index = Some(combat.damage_step_index.unwrap_or(0) + 1);
    }

    if let Some(waiting_for) = super::combat_damage::resolve_combat_damage(state, events) {
        return Ok(waiting_for);
    }

    priority::reset_priority(state);
    Ok(WaitingFor::Priority { player })
}

pub(super) fn handle_empty_attackers(
    state: &mut GameState,
    events: &mut Vec<GameEvent>,
) -> Result<WaitingFor, EngineError> {
    super::combat::declare_attackers(state, &[], events).map_err(EngineError::InvalidAction)?;

    triggers::process_triggers(state, events);
    if let Some(waiting_for) = begin_pending_trigger_target_selection(state)? {
        return Ok(waiting_for);
    }

    state.phase = Phase::EndCombat;
    events.push(GameEvent::PhaseChanged {
        phase: Phase::EndCombat,
    });
    state.combat = None;
    super::layers::prune_end_of_combat_effects(state);
    turns::advance_phase(state, events);
    Ok(turns::auto_advance(state, events))
}

pub(super) fn handle_empty_blockers(
    state: &mut GameState,
    events: &mut Vec<GameEvent>,
) -> Result<WaitingFor, EngineError> {
    super::combat::declare_blockers(state, &[], events).map_err(EngineError::InvalidAction)?;

    triggers::process_triggers(state, events);
    if let Some(waiting_for) = begin_pending_trigger_target_selection(state)? {
        return Ok(waiting_for);
    }

    turns::advance_phase(state, events);
    Ok(turns::auto_advance(state, events))
}
