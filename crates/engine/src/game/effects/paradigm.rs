use crate::types::events::GameEvent;
use crate::types::game_state::{ExileLink, ExileLinkKind, GameState, ParadigmPrime, WaitingFor};
use crate::types::identifiers::ObjectId;
use crate::types::player::PlayerId;

/// CR 702.xxx: Paradigm (Strixhaven) — first-resolution hook. Called from
/// `stack.rs` when a spell with `Keyword::Paradigm` successfully resolves for
/// the first time by its controller (per card name).
///
/// Action: (a) push `(controller, card_name)` to `state.paradigm_primed`,
/// (b) override the spell's post-resolve destination to exile (CR 608.2n is
/// displaced by the Paradigm reminder text), (c) create an `ExileLink` with
/// `ExileLinkKind::ParadigmSource { player: controller }` pointing to the
/// spell object. Returns `true` if the hook fired (caller should skip the
/// default graveyard-routing branch). Assign when WotC publishes SOS CR
/// update.
///
/// The exiled card is the original spell object (it is left on the stack at
/// the point the resolver inspects it; the stack.rs caller moves it to exile
/// after `arm_paradigm` returns true instead of to the graveyard).
pub fn arm_paradigm(
    state: &mut GameState,
    object_id: ObjectId,
    controller: PlayerId,
    card_name: &str,
) -> bool {
    // CR 702.xxx: "After you first resolve a spell with this name" — gate on
    // (player, card_name). Already-primed spells follow default routing.
    let already_primed = state
        .paradigm_primed
        .iter()
        .any(|p| p.player == controller && p.card_name.eq_ignore_ascii_case(card_name));
    if already_primed {
        return false;
    }
    state.paradigm_primed.push(ParadigmPrime {
        player: controller,
        card_name: card_name.to_string(),
    });
    state.exile_links.push(ExileLink {
        source_id: object_id,
        exiled_id: object_id,
        kind: ExileLinkKind::ParadigmSource { player: controller },
    });
    true
}

/// CR 702.xxx: Paradigm (Strixhaven) — turn-based offer scan. Called from
/// `turns.rs` at the start of the active player's first precombat main phase
/// (CR 505.3 precedent for beginning-of-main turn-based actions). Returns the
/// list of exiled paradigm sources that belong to the given player. Assign
/// when WotC publishes SOS CR update.
pub fn paradigm_offers_for(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
    state
        .exile_links
        .iter()
        .filter_map(|link| match link.kind {
            ExileLinkKind::ParadigmSource { player: owner } if owner == player => {
                Some(link.exiled_id)
            }
            _ => None,
        })
        .collect()
}

/// Enqueue a `WaitingFor::ParadigmCastOffer` if offers exist for the given
/// player. Returns true if a `WaitingFor` was set; false if no offers and the
/// caller should continue normal phase flow.
pub fn enqueue_offer_if_any(state: &mut GameState, player: PlayerId) -> bool {
    let offers = paradigm_offers_for(state, player);
    if offers.is_empty() {
        return false;
    }
    state.waiting_for = WaitingFor::ParadigmCastOffer { player, offers };
    true
}

/// CR 702.xxx + CR 707.10f: Build a token spell-copy on the stack from an
/// exiled paradigm source. The exiled card stays in exile; the copy is a
/// fresh ObjectId, `is_token = true`, `CastingVariant::Normal`, controller =
/// acting player. Returns Ok(copy_id) on success. Assign when WotC publishes
/// SOS CR update.
pub fn cast_paradigm_copy(
    state: &mut GameState,
    source_id: ObjectId,
    controller: PlayerId,
    events: &mut Vec<GameEvent>,
) -> Result<ObjectId, String> {
    use crate::types::ability::{ResolvedAbility, TargetRef};
    use crate::types::game_state::{CastingVariant, StackEntry, StackEntryKind};
    use crate::types::zones::Zone;

    let (src_clone, card_id) = {
        let Some(src_obj) = state.objects.get(&source_id) else {
            return Err(format!("paradigm source {source_id:?} not found"));
        };
        (src_obj.clone(), src_obj.card_id)
    };
    // Verify this is an exiled paradigm source owned by the acting player.
    let has_link = state.exile_links.iter().any(|link| {
        link.exiled_id == source_id
            && matches!(link.kind, ExileLinkKind::ParadigmSource { player } if player == controller)
    });
    if !has_link {
        return Err("no ParadigmSource link for this source/player".to_string());
    }
    // Select the first ability as the spell ability.
    let ability_def = src_clone
        .abilities
        .first()
        .cloned()
        .ok_or_else(|| "paradigm source has no spell ability".to_string())?;

    let copy_id = ObjectId(state.next_object_id);
    state.next_object_id += 1;

    let mut copy_obj = src_clone;
    copy_obj.id = copy_id;
    copy_obj.controller = controller;
    copy_obj.owner = controller;
    copy_obj.zone = Zone::Stack;
    copy_obj.is_token = true;
    copy_obj.tapped = false;
    copy_obj.prepared = None;
    // Back-face is preserved from clone — not needed for copy behavior.
    state.objects.insert(copy_id, copy_obj);

    let resolved = ResolvedAbility::new(
        *ability_def.effect.clone(),
        Vec::<TargetRef>::new(),
        copy_id,
        controller,
    );

    state.stack.push(StackEntry {
        id: copy_id,
        source_id: copy_id,
        controller,
        kind: StackEntryKind::Spell {
            card_id,
            ability: Some(resolved),
            casting_variant: CastingVariant::Normal,
            actual_mana_spent: 0,
        },
    });
    events.push(GameEvent::StackPushed { object_id: copy_id });

    Ok(copy_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arm_paradigm_primes_once_per_name() {
        let mut state = GameState::new_two_player(42);
        let obj = ObjectId(100);
        let p = PlayerId(0);
        assert!(arm_paradigm(&mut state, obj, p, "Restoration Seminar"));
        assert_eq!(state.paradigm_primed.len(), 1);
        assert_eq!(state.exile_links.len(), 1);

        // Second resolution with same name for same player does not re-prime.
        let obj2 = ObjectId(101);
        assert!(!arm_paradigm(&mut state, obj2, p, "Restoration Seminar"));
        assert_eq!(state.paradigm_primed.len(), 1);
        assert_eq!(state.exile_links.len(), 1);

        // Different player can prime the same name separately.
        let p2 = PlayerId(1);
        assert!(arm_paradigm(&mut state, obj2, p2, "Restoration Seminar"));
        assert_eq!(state.paradigm_primed.len(), 2);
        assert_eq!(state.exile_links.len(), 2);
    }

    #[test]
    fn offers_scoped_to_player() {
        let mut state = GameState::new_two_player(42);
        arm_paradigm(&mut state, ObjectId(100), PlayerId(0), "Foo");
        arm_paradigm(&mut state, ObjectId(101), PlayerId(1), "Bar");
        assert_eq!(
            paradigm_offers_for(&state, PlayerId(0)),
            vec![ObjectId(100)]
        );
        assert_eq!(
            paradigm_offers_for(&state, PlayerId(1)),
            vec![ObjectId(101)]
        );
    }
}
