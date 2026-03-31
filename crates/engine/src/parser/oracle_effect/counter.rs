use nom::Parser;

use crate::types::ability::{DoublePTMode, DoubleTarget, Effect, MultiTargetSpec, TargetFilter};
use crate::types::mana::ManaColor;

use super::super::oracle_nom::primitives as nom_primitives;
use super::super::oracle_target::{parse_target, parse_type_phrase};
use super::super::oracle_util::{parse_count_expr, parse_number};
use super::{resolve_it_pronoun, ParseContext};

pub(super) fn try_parse_put_counter<'a>(
    lower: &str,
    text: &'a str,
    ctx: &ParseContext,
) -> Option<(Effect, &'a str, Option<MultiTargetSpec>)> {
    // "put N {type} counter(s) on {target}"
    // Use parse_count_expr to handle Variable("X") for kicker-X patterns.
    let after_put = lower[4..].trim();
    let (count_expr, rest) = parse_count_expr(after_put)?;
    // Next word is counter type (e.g. "+1/+1", "loyalty", "charge")
    let type_end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
    let raw_type = &rest[..type_end];
    let counter_type = normalize_counter_type(raw_type);

    // Skip "counter" or "counters" keyword, then parse target after "on"
    let after_type = rest[type_end..].trim_start();
    let after_counter_word = after_type
        .strip_prefix("counters")
        .or_else(|| after_type.strip_prefix("counter"))
        .map(|s| s.trim_start())
        .unwrap_or(after_type);

    let (target, remainder, multi_target) = if let Some(on_rest) =
        after_counter_word.strip_prefix("on ")
    {
        if on_rest.strip_prefix("this ").is_some() || on_rest.strip_prefix("~").is_some() {
            // Explicit self-reference — always SelfRef
            (TargetFilter::SelfRef, "", None)
        } else if on_rest == "it"
            || on_rest.strip_prefix("it ").is_some()
            || on_rest.strip_prefix("itself").is_some()
        {
            // CR 608.2k: Bare pronoun — context-dependent
            (resolve_it_pronoun(ctx), "", None)
        } else {
            // CR 115.1d: Strip "up to N" quantifier before target parsing.
            // "put a +1/+1 counter on up to one target creature" — the "up to N"
            // modifies the target count, not the counter count.
            let (target_text, multi) = if let Some(after_up_to) = on_rest.strip_prefix("up to ") {
                if let Some((n, after_n)) = parse_number(after_up_to) {
                    let on_offset = lower.len() - after_n.len();
                    (
                        &text[on_offset..],
                        Some(MultiTargetSpec {
                            min: 0,
                            max: Some(n as usize),
                        }),
                    )
                } else {
                    let on_offset = lower.len() - on_rest.len();
                    (&text[on_offset..], None)
                }
            } else if let Some(after_each_of) = on_rest.strip_prefix("each of up to ") {
                if let Some((n, after_n)) = parse_number(after_each_of) {
                    let on_offset = lower.len() - after_n.len();
                    (
                        &text[on_offset..],
                        Some(MultiTargetSpec {
                            min: 0,
                            max: Some(n as usize),
                        }),
                    )
                } else {
                    let on_offset = lower.len() - on_rest.len();
                    (&text[on_offset..], None)
                }
            } else {
                let on_offset = lower.len() - on_rest.len();
                (&text[on_offset..], None)
            };

            let (target, rem) = parse_target(target_text);
            (target, rem, multi)
        }
    } else {
        (TargetFilter::SelfRef, "", None)
    };

    Some((
        Effect::PutCounter {
            counter_type,
            count: count_expr,
            target,
        },
        remainder,
        multi_target,
    ))
}

pub(super) fn try_parse_remove_counter(lower: &str, ctx: &ParseContext) -> Option<Effect> {
    // "remove N {type} counter(s) from {target}"
    let after_remove = lower[7..].trim();
    let (count, rest) = parse_number(after_remove)?;
    let type_end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
    let raw_type = &rest[..type_end];
    let counter_type = normalize_counter_type(raw_type);

    let after_type = rest[type_end..].trim_start();
    let after_counter_word = after_type
        .strip_prefix("counters")
        .or_else(|| after_type.strip_prefix("counter"))
        .map(|s| s.trim_start())?;

    let target_text = after_counter_word.strip_prefix("from ")?.trim();
    let target =
        if target_text.strip_prefix("this ").is_some() || target_text.strip_prefix("~").is_some() {
            TargetFilter::SelfRef
        } else if target_text == "it"
            || target_text.strip_prefix("it ").is_some()
            || target_text.strip_prefix("itself").is_some()
        {
            // CR 608.2k: Bare pronoun — context-dependent
            resolve_it_pronoun(ctx)
        } else {
            let (t, _rem) = parse_target(target_text);
            #[cfg(debug_assertions)]
            super::types::assert_no_compound_remainder(_rem, target_text);
            t
        };

    Some(Effect::RemoveCounter {
        counter_type,
        count: count as i32,
        target,
    })
}

/// Normalize oracle-text counter type strings to canonical engine names.
pub(crate) fn normalize_counter_type(raw: &str) -> String {
    match raw {
        "+1/+1" => "P1P1".to_string(),
        "-1/-1" => "M1M1".to_string(),
        other => other.to_string(),
    }
}

/// CR 121.5: Parse "put its counters on [target]" → MoveCounters effect.
/// "its" / "this creature's" are possessive pronouns referring to the ability source.
pub(super) fn try_parse_move_counters<'a>(lower: &str, text: &'a str) -> Option<(Effect, &'a str)> {
    let after_put = lower.strip_prefix("put ")?.trim();
    // Detect "its counters" / "this creature's counters"
    let after_possessive = after_put
        .strip_prefix("its counter")
        .or_else(|| after_put.strip_prefix("this creature's counter"))?;
    // Skip past optional "s" (counter vs counters) then expect " on "
    let after_counters = after_possessive
        .strip_prefix('s')
        .unwrap_or(after_possessive);
    let after_on = after_counters.strip_prefix(" on ")?;

    // Compute byte offset into original `text` for parse_target.
    let offset_in_text = text.len() - after_on.len();
    let (target, remainder) = parse_target(&text[offset_in_text..]);

    Some((
        Effect::MoveCounters {
            source: TargetFilter::SelfRef,
            counter_type: None,
            target,
        },
        remainder,
    ))
}

/// CR 701.10e: Parse "double the number of {type} counters on {target}".
pub(super) fn try_parse_multiply_counter(lower: &str, ctx: &ParseContext) -> Option<Effect> {
    let rest = lower.strip_prefix("double the number of ")?;
    // Parse counter type — next word(s) before "counter(s)"
    let type_end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
    let raw_type = &rest[..type_end];
    let counter_type = normalize_counter_type(raw_type);

    // Skip counter type + "counter(s) on "
    let after_type = rest[type_end..].trim_start();
    let after_counter_word = after_type
        .strip_prefix("counters")
        .or_else(|| after_type.strip_prefix("counter"))
        .map(|s| s.trim_start())?;
    let target_text = after_counter_word.strip_prefix("on ")?;

    let target =
        if target_text.strip_prefix("this ").is_some() || target_text.strip_prefix("~").is_some() {
            TargetFilter::SelfRef
        } else if target_text == "it"
            || target_text.strip_prefix("it ").is_some()
            || target_text.strip_prefix("itself").is_some()
        {
            // CR 608.2k: Bare pronoun — context-dependent
            resolve_it_pronoun(ctx)
        } else {
            let (t, _rem) = parse_target(target_text);
            #[cfg(debug_assertions)]
            super::types::assert_no_compound_remainder(_rem, target_text);
            t
        };

    Some(Effect::MultiplyCounter {
        counter_type,
        multiplier: 2,
        target,
    })
}

/// CR 701.10: Dispatch "double the ..." to counter-doubling, life-doubling,
/// mana-doubling, or P/T-doubling.
pub(super) fn try_parse_double_effect(lower: &str, ctx: &ParseContext) -> Option<Effect> {
    // CR 701.10e: "double the number of each kind of counter on ..." → all counter types
    if let Some(rest) = lower.strip_prefix("double the number of each kind of counter on ") {
        let target = if rest.strip_prefix("target ").is_some() {
            let (t, _rem) = parse_target(rest);
            #[cfg(debug_assertions)]
            super::types::assert_no_compound_remainder(_rem, rest);
            t
        } else if rest.strip_prefix("~").is_some() || rest.strip_prefix("this ").is_some() {
            TargetFilter::SelfRef
        } else if rest.strip_prefix("it").is_some() {
            // CR 608.2k: Bare pronoun — context-dependent
            resolve_it_pronoun(ctx)
        } else {
            let (t, _rem) = parse_target(rest);
            #[cfg(debug_assertions)]
            super::types::assert_no_compound_remainder(_rem, rest);
            t
        };
        return Some(Effect::Double {
            target_kind: DoubleTarget::Counters { counter_type: None },
            target,
        });
    }

    // Counter doubling: "double the number of ..."
    if lower.strip_prefix("double the number of ").is_some() {
        return try_parse_multiply_counter(lower, ctx);
    }

    // CR 701.10d: "double your life total" / "double target player's life total"
    if let Some(rest) = lower.strip_prefix("double ") {
        if rest.strip_prefix("your life total").is_some() {
            return Some(Effect::Double {
                target_kind: DoubleTarget::LifeTotal,
                target: TargetFilter::Controller,
            });
        }
        if rest.strip_prefix("target ").is_some() && rest.contains("life total") {
            let (target, _) = parse_target(rest);
            return Some(Effect::Double {
                target_kind: DoubleTarget::LifeTotal,
                target,
            });
        }
    }

    // CR 701.10f: "double the amount of {color} mana in your mana pool"
    if let Some(rest) = lower.strip_prefix("double the amount of ") {
        if rest.contains("mana") {
            let color = parse_mana_color_from_text(rest);
            return Some(Effect::Double {
                target_kind: DoubleTarget::ManaPool { color },
                target: TargetFilter::Controller,
            });
        }
    }

    // CR 608.2k: "double its power [and toughness]" — possessive "its" is context-dependent
    if let Some(rest) = lower.strip_prefix("double its ") {
        let mode = if rest.strip_prefix("power and toughness").is_some() {
            DoublePTMode::PowerAndToughness
        } else if rest.strip_prefix("power").is_some() {
            DoublePTMode::Power
        } else if rest.strip_prefix("toughness").is_some() {
            DoublePTMode::Toughness
        } else {
            return None;
        };
        return Some(Effect::DoublePT {
            mode,
            target: resolve_it_pronoun(ctx),
        });
    }

    // P/T doubling: "double the power/toughness [and toughness/power] of ..."
    let rest = lower.strip_prefix("double the ")?;

    let (mode, after_mode) = if let Some(r) = rest.strip_prefix("power and toughness of ") {
        (DoublePTMode::PowerAndToughness, r)
    } else if let Some(r) = rest.strip_prefix("power of ") {
        (DoublePTMode::Power, r)
    } else if let Some(r) = rest.strip_prefix("toughness of ") {
        (DoublePTMode::Toughness, r)
    } else {
        return None;
    };

    // "target creature you control" → targeted DoublePT
    if after_mode.strip_prefix("target ").is_some() {
        let (target, _) = parse_target(after_mode);
        return Some(Effect::DoublePT { mode, target });
    }

    // "each creature you control" / "each other creature" / "each Dragon" → DoublePTAll
    let filter_text = after_mode
        .strip_prefix("each ")
        .or_else(|| after_mode.strip_prefix("all "))?;
    let (target, _) = parse_type_phrase(filter_text);
    Some(Effect::DoublePTAll { mode, target })
}

/// Parse a mana color name from text like "red mana in your mana pool".
///
/// Delegates to `nom_primitives::parse_color` for color word recognition.
fn parse_mana_color_from_text(text: &str) -> Option<ManaColor> {
    let lower = text.split_whitespace().next()?.to_lowercase();
    let (_rest, color) = nom_primitives::parse_color.parse(&lower).ok()?;
    Some(color)
}
