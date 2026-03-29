use engine::game::game_object::GameObject;
use engine::game::players;
use engine::types::card_type::CoreType;
use engine::types::game_state::GameState;
use engine::types::keywords::Keyword;
use engine::types::phase::Phase;
use engine::types::player::PlayerId;

use crate::cast_facts::cast_facts_for_action;
use crate::eval::{evaluate_creature, threat_level};

use super::context::PolicyContext;

pub(crate) fn is_own_main_phase(ctx: &PolicyContext<'_>) -> bool {
    ctx.state.active_player == ctx.ai_player
        && ctx.state.stack.is_empty()
        && matches!(
            ctx.state.phase,
            Phase::PreCombatMain | Phase::PostCombatMain
        )
}

pub(crate) fn board_presence_score(object: &GameObject) -> f64 {
    let mut score = 0.0;

    if object.card_types.core_types.contains(&CoreType::Creature) {
        let power = object.power.unwrap_or(0).max(0) as f64;
        let toughness = object.toughness.unwrap_or(0).max(0) as f64;
        score += ((power + toughness) / 8.0).min(0.45);
        score += keyword_pressure(object) * 0.04;
    } else if object
        .card_types
        .core_types
        .contains(&CoreType::Planeswalker)
    {
        score += 0.28 + object.loyalty.unwrap_or(0) as f64 / 20.0;
    } else if object.card_types.core_types.iter().any(|core_type| {
        matches!(
            core_type,
            CoreType::Artifact | CoreType::Battle | CoreType::Enchantment
        )
    }) {
        score += 0.16;
    }

    score.min(0.65)
}

pub(crate) fn best_proactive_cast_score(ctx: &PolicyContext<'_>) -> f64 {
    ctx.decision
        .candidates
        .iter()
        .filter_map(|candidate| cast_facts_for_action(ctx.state, &candidate.action, ctx.ai_player))
        .map(|facts| {
            let mut score = board_presence_score(facts.object);
            if !facts.immediate_etb_triggers.is_empty() || !facts.immediate_replacements.is_empty()
            {
                score += 0.16;
            }
            if facts.has_search_library {
                score += 0.24;
            }
            if facts.has_draw {
                score += 0.1;
            }
            if facts.has_direct_removal_text {
                score += 0.14;
            }
            score
        })
        .fold(0.0, f64::max)
}

pub(crate) fn visible_opponent_creature_value(state: &GameState, ai_player: PlayerId) -> f64 {
    let opponents = players::opponents(state, ai_player);
    state
        .battlefield
        .iter()
        .filter_map(|object_id| {
            let object = state.objects.get(object_id)?;
            if opponents.contains(&object.controller)
                && object.card_types.core_types.contains(&CoreType::Creature)
            {
                Some(
                    evaluate_creature(state, *object_id)
                        * (threat_level(state, ai_player, object.controller) + 0.5),
                )
            } else {
                None
            }
        })
        .fold(0.0, f64::max)
}

pub(crate) fn battlefield_pressure_delta(state: &GameState, ai_player: PlayerId) -> f64 {
    let mut ours = 0.0;
    let mut theirs = 0.0;

    for object_id in &state.battlefield {
        let Some(object) = state.objects.get(object_id) else {
            continue;
        };
        if !object.card_types.core_types.contains(&CoreType::Creature) {
            continue;
        }
        let value = evaluate_creature(state, *object_id);
        if object.controller == ai_player {
            ours += value;
        } else {
            theirs += value;
        }
    }

    ours - theirs
}

fn keyword_pressure(object: &GameObject) -> f64 {
    object
        .keywords
        .iter()
        .map(|keyword| match keyword {
            Keyword::Flying
            | Keyword::Trample
            | Keyword::Vigilance
            | Keyword::Menace
            | Keyword::Lifelink
            | Keyword::Deathtouch
            | Keyword::FirstStrike
            | Keyword::DoubleStrike
            | Keyword::Haste => 1.0,
            _ => 0.0,
        })
        .sum::<f64>()
        .min(3.0)
}
