//! Shared helpers for walking an `AbilityDefinition`'s effect chain.
//!
//! `AbilityDefinition` composes a primary `effect` with an optional
//! `sub_ability` that itself has an `effect` plus another optional
//! `sub_ability`, forming a single-linked list. Feature detectors and
//! policies both need to classify the *set* of effects produced by an
//! ability (e.g., "does this ability both search the library and put a
//! land onto the battlefield?"), so they collect the chain into a flat
//! slice and iterate with `matches!`.
//!
//! Keep this module small — it is a single building block shared across
//! `features/*` and `policies/*`.

use engine::types::ability::{AbilityDefinition, Effect};

/// Walk `ability.effect` plus each `sub_ability.effect` in turn, returning
/// borrowed references in chain order.
pub(crate) fn collect_chain_effects(ability: &AbilityDefinition) -> Vec<&Effect> {
    let mut effects: Vec<&Effect> = vec![&ability.effect];
    let mut current = &ability.sub_ability;
    while let Some(sub) = current {
        effects.push(&sub.effect);
        current = &sub.sub_ability;
    }
    effects
}
