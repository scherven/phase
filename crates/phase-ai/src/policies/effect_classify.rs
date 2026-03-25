use engine::game::game_object::GameObject;
use engine::types::ability::{ContinuousModification, Effect, PtValue, TargetFilter, TypeFilter};
use engine::types::statics::StaticMode;
use engine::types::zones::Zone;

use super::context::PolicyContext;

/// Three-valued polarity: whether an effect benefits or harms its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EffectPolarity {
    /// Target benefits (pump, regenerate, +1/+1 counters, untap, animate)
    Beneficial,
    /// Target is harmed (destroy, damage, -1/-1 counters, sacrifice)
    Harmful,
    /// Depends on context — fall through to default "assume harmful" behavior
    Contextual,
}

pub(crate) fn effect_polarity(effect: &Effect) -> EffectPolarity {
    match effect {
        // Pump: beneficial only if both values are non-negative
        Effect::Pump {
            power, toughness, ..
        } => {
            let p_ok = matches!(power, PtValue::Fixed(v) if *v >= 0)
                || matches!(power, PtValue::Variable(_) | PtValue::Quantity(_));
            let t_ok = matches!(toughness, PtValue::Fixed(v) if *v >= 0)
                || matches!(toughness, PtValue::Variable(_) | PtValue::Quantity(_));
            if p_ok && t_ok {
                EffectPolarity::Beneficial
            } else {
                EffectPolarity::Harmful
            }
        }
        // Counters: +1/+1 is beneficial, -1/-1 is harmful
        Effect::AddCounter { counter_type, .. } => {
            if counter_type.starts_with('+') {
                EffectPolarity::Beneficial
            } else if counter_type.starts_with('-') {
                EffectPolarity::Harmful
            } else {
                EffectPolarity::Contextual
            }
        }
        Effect::Regenerate { .. }
        | Effect::PreventDamage { .. }
        | Effect::Animate { .. }
        | Effect::DoublePT { .. } => EffectPolarity::Beneficial,
        Effect::Untap { .. } => EffectPolarity::Beneficial,
        // Beneficial: resource generation and card advantage
        Effect::GainLife { .. }
        | Effect::Draw { .. }
        | Effect::Token { .. }
        | Effect::Scry { .. }
        | Effect::Explore
        | Effect::Investigate
        | Effect::Mana { .. }
        | Effect::SearchLibrary { .. }
        | Effect::Surveil { .. }
        | Effect::Connive { .. }
        | Effect::BecomeMonarch
        | Effect::ExtraTurn { .. } => EffectPolarity::Beneficial,
        // Harmful: removal, disruption, and forced actions
        Effect::Destroy { .. }
        | Effect::DealDamage { .. }
        | Effect::Sacrifice { .. }
        | Effect::DiscardCard { .. }
        | Effect::Mill { .. }
        | Effect::LoseLife { .. }
        | Effect::RemoveCounter { .. }
        | Effect::Tap { .. }
        | Effect::Bounce { .. }
        | Effect::Counter { .. }
        | Effect::PhaseOut { .. }
        | Effect::Fight { .. }
        | Effect::Goad { .. }
        | Effect::ForceBlock { .. }
        | Effect::DestroyAll { .. }
        | Effect::DamageAll { .. }
        | Effect::LoseTheGame => EffectPolarity::Harmful,
        // ChangeZone: depends on destination
        Effect::ChangeZone { destination, .. } => match destination {
            Zone::Exile | Zone::Graveyard => EffectPolarity::Harmful,
            Zone::Battlefield => EffectPolarity::Beneficial,
            _ => EffectPolarity::Contextual,
        },
        // Contextual: depends on usage context
        Effect::GainControl { .. }
        | Effect::GiftDelivery { .. }
        | Effect::Suspect { .. }
        | Effect::GivePlayerCounter { .. }
        | Effect::ExchangeControl => EffectPolarity::Contextual,
        _ => EffectPolarity::Contextual,
    }
}

/// Extract the target filter from an effect, if present.
pub(crate) fn extract_target_filter(effect: &Effect) -> Option<&TargetFilter> {
    match effect {
        // Beneficial effects
        Effect::Pump { target, .. }
        | Effect::AddCounter { target, .. }
        | Effect::Animate { target, .. }
        | Effect::DoublePT { target, .. }
        | Effect::Regenerate { target, .. }
        | Effect::Untap { target }
        | Effect::PreventDamage { target, .. }
        // Harmful effects
        | Effect::Destroy { target, .. }
        | Effect::DealDamage { target, .. }
        | Effect::Tap { target }
        | Effect::RemoveCounter { target, .. }
        // Removal / disruption
        | Effect::Bounce { target, .. }
        | Effect::Counter { target, .. }
        | Effect::GainControl { target, .. }
        | Effect::PhaseOut { target }
        | Effect::Fight { target, .. }
        | Effect::Goad { target }
        | Effect::ChangeZone { target, .. }
        | Effect::Connive { target, .. }
        | Effect::Suspect { target, .. }
        | Effect::ForceBlock { target, .. }
        | Effect::Exploit { target, .. }
        | Effect::Attach { target, .. }
        | Effect::GivePlayerCounter { target, .. }
        | Effect::BecomeCopy { target, .. }
        | Effect::ExtraTurn { target, .. } => Some(target),
        // NOTE: ExchangeControl is a unit variant — no target field.
        // NOTE: GiftDelivery { kind } has no target field.
        // NOTE: SearchLibrary uses `filter`, not `target`.
        _ => None,
    }
}

/// Returns true if the effect exclusively targets creatures (not "any target").
/// Used for harmful spells: burn with TargetFilter::Any can still go face.
pub(crate) fn targets_creatures_only(effect: &Effect) -> bool {
    let filter = extract_target_filter(effect);
    matches!(
        filter,
        Some(TargetFilter::Typed(typed))
            if typed.type_filters.iter().any(|t| matches!(t, TypeFilter::Creature))
    )
}

/// Returns true if an effect's target filter is creature-typed (or Any).
pub(crate) fn targets_creatures(effect: &Effect) -> bool {
    let Some(filter) = extract_target_filter(effect) else {
        return false;
    };
    match filter {
        TargetFilter::Any => true,
        TargetFilter::Typed(typed) => typed
            .type_filters
            .iter()
            .any(|t| matches!(t, TypeFilter::Creature)),
        _ => false,
    }
}

/// Returns true if the pending spell's dominant effect is beneficial to its target.
/// Defaults to false (assume harmful) when uncertain — safe fallback since most
/// targeted spells in MTG are removal/damage.
pub(crate) fn is_spell_beneficial(ctx: &PolicyContext<'_>) -> bool {
    let effects = ctx.effects();

    // Check active effects for a clear polarity signal.
    let dominant_polarity = effects.first().map(|e| effect_polarity(e));
    match dominant_polarity {
        Some(EffectPolarity::Beneficial) => return true,
        Some(EffectPolarity::Harmful) => return false,
        _ => {}
    }

    // No clear polarity from active effects (empty or Contextual).
    // Auras carry their beneficial/harmful nature in static definitions.
    if let Some(source) = ctx.source_object() {
        if source.card_types.subtypes.iter().any(|s| s == "Aura") {
            return matches!(aura_polarity(source), EffectPolarity::Beneficial);
        }
    }

    false
}

/// Determines whether an Aura is beneficial or harmful to its target by inspecting
/// both static modes (CantAttack, CantBeBlocked, etc.) and continuous modifications.
pub(crate) fn aura_polarity(source: &GameObject) -> EffectPolarity {
    // First check static modes — these carry clear polarity independent of modifications.
    for sd in &source.static_definitions {
        match static_mode_polarity(&sd.mode) {
            EffectPolarity::Contextual => continue,
            polarity => return polarity,
        }
    }

    // Then check continuous modifications (AddPower, AddKeyword, etc.).
    for sd in &source.static_definitions {
        for m in &sd.modifications {
            match modification_polarity(m) {
                EffectPolarity::Contextual => continue,
                polarity => return polarity,
            }
        }
    }

    EffectPolarity::Contextual
}

/// Classify a static mode as beneficial/harmful to the enchanted permanent.
pub(crate) fn static_mode_polarity(mode: &StaticMode) -> EffectPolarity {
    match mode {
        // Harmful: restricts the enchanted permanent
        StaticMode::CantAttack
        | StaticMode::CantBlock
        | StaticMode::CantUntap
        | StaticMode::MustAttack
        | StaticMode::MustBlock
        | StaticMode::CantGainLife
        | StaticMode::CantBeActivated => EffectPolarity::Harmful,
        // Beneficial: enhances the enchanted permanent
        StaticMode::CantBeBlocked
        | StaticMode::CantBeBlockedExceptBy { .. }
        | StaticMode::CantBeTargeted
        | StaticMode::CantBeCountered
        | StaticMode::Protection
        | StaticMode::CastWithFlash => EffectPolarity::Beneficial,
        // Continuous, cost changes, and others depend on modifications/context
        _ => EffectPolarity::Contextual,
    }
}

/// Classify a continuous modification as beneficial/harmful to its target.
pub(crate) fn modification_polarity(m: &ContinuousModification) -> EffectPolarity {
    match m {
        ContinuousModification::AddPower { value }
        | ContinuousModification::AddToughness { value } => {
            if *value > 0 {
                EffectPolarity::Beneficial
            } else if *value < 0 {
                EffectPolarity::Harmful
            } else {
                EffectPolarity::Contextual
            }
        }
        ContinuousModification::AddDynamicPower { .. }
        | ContinuousModification::AddDynamicToughness { .. } => EffectPolarity::Beneficial,
        ContinuousModification::AddKeyword { .. }
        | ContinuousModification::GrantAbility { .. }
        | ContinuousModification::AddAllCreatureTypes
        | ContinuousModification::AddColor { .. }
        | ContinuousModification::AddType { .. }
        | ContinuousModification::AddSubtype { .. } => EffectPolarity::Beneficial,
        ContinuousModification::RemoveKeyword { .. }
        | ContinuousModification::RemoveAllAbilities
        | ContinuousModification::RemoveType { .. }
        | ContinuousModification::RemoveSubtype { .. } => EffectPolarity::Harmful,
        // SetPower/SetToughness, SetColor, etc. are contextual — could go either way.
        _ => EffectPolarity::Contextual,
    }
}
