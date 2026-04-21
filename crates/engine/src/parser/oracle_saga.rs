use std::collections::HashSet;

use nom::branch::alt;
use nom::bytes::complete::tag;
use nom::Parser;
use nom_language::error::VerboseError;

use crate::types::ability::{
    AbilityDefinition, AbilityKind, CounterTriggerFilter, Effect, QuantityExpr,
    ReplacementDefinition, TargetFilter, TriggerDefinition,
};
use crate::types::replacements::ReplacementEvent;
use crate::types::triggers::TriggerMode;
use crate::types::zones::Zone;

use super::oracle_effect::parse_effect_chain;
use super::oracle_nom::primitives as nom_primitives;
use super::oracle_util::strip_reminder_text;

/// Parse a roman numeral to u32. Handles I(1) through XX(20).
///
/// Delegates to the shared `nom_primitives::parse_roman_numeral` combinator,
/// but requires the entire input to be a roman numeral (no trailing non-roman text).
pub(crate) fn parse_roman_numeral(s: &str) -> Option<u32> {
    let (rest, val) = nom_primitives::parse_roman_numeral(s).ok()?;
    // The original function required the entire string to be a roman numeral.
    // The nom combinator consumes all roman chars, so verify nothing else remains.
    if !rest.is_empty() {
        return None;
    }
    Some(val)
}

/// Parse a saga chapter line. Returns (chapter_numbers, effect_text).
/// Handles "I — effect", "I, II — effect", "III, IV, V — effect" (arbitrary-length lists).
///
/// Also strips the optional flavor-name (chapter title) interjection used on FIN
/// Summon sagas, FIN warden sagas, Weatherseed Treaty, etc.:
/// `"I — Crescent Fang — Search your library…"` → effect = `"Search your library…"`.
pub(crate) fn parse_chapter_line(line: &str) -> Option<(Vec<u32>, String)> {
    // Split the line around the first chapter-separator (em dash preferred, hyphen fallback).
    let (prefix, effect) = split_on_chapter_separator(line)?;

    let nums: Vec<u32> = prefix
        .split(',')
        .filter_map(|part| parse_roman_numeral(part.trim()))
        .collect();

    if nums.is_empty() {
        return None;
    }

    Some((nums, strip_chapter_title(effect.trim()).to_string()))
}

/// Split a chapter line on its first chapter-separator (em dash `" — "` or hyphen
/// fallback `" - "`), returning `(prefix_before_separator, body_after_separator)`.
///
/// Uses `take_until` + `alt(tag,tag)` so the separator alternatives live in a single
/// composable combinator with structured `VerboseError` diagnostics, rather than
/// chained `split_once` calls.
fn split_on_chapter_separator(line: &str) -> Option<(&str, &str)> {
    for sep in [" — ", " - "] {
        let parse = nom::bytes::complete::take_until::<_, _, VerboseError<&str>>(sep).and(tag::<
            _,
            _,
            VerboseError<&str>,
        >(
            sep
        ));
        let mut parser = parse;
        if let Ok((body, (prefix, _))) = parser.parse(line) {
            return Some((prefix, body));
        }
    }
    None
}

/// Strip an optional chapter-title flavor-name prefix from a saga chapter effect.
///
/// Chapter titles (e.g. `"Crescent Fang"`, `"Jecht Beam"`, `"Domain"`) are purely
/// flavorful and have no game meaning. They appear as `"<Title> — <effect>"`
/// inside the chapter body, separated from the actual rules text by another em-dash.
///
/// Recognized by structure, not a name list: the prefix must be short, capitalized,
/// and free of sentence punctuation. Any effect that naturally contains an em-dash
/// would be highly unusual in Oracle text.
fn strip_chapter_title(effect: &str) -> &str {
    let Some((title, body)) = split_on_chapter_separator(effect) else {
        return effect;
    };
    let title = title.trim();
    let looks_like_title = !title.is_empty()
        && title.len() < 40
        && title.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && !title.contains(['.', ',', ';', ':', '!', '?']);
    if looks_like_title {
        body.trim()
    } else {
        effect
    }
}

/// Returns `true` if `line` is a bullet-list continuation of the previous chapter's
/// body (e.g. a `"• Option A"` entry under a `"Choose one —"` chapter).
///
/// Trailing keyword lines (`"Flying"`, `"Menace"`, `"Trample, haste"`) on FIN Summon
/// sagas and Weatherseed-era Wardens are *not* continuations — they belong to the
/// creature's keyword set and must flow through the general dispatcher's keyword
/// extractor (priority 1b in `oracle.rs`).
fn is_chapter_body_continuation(line: &str) -> bool {
    let result: nom::IResult<&str, &str, VerboseError<&str>> =
        alt((tag("•"), tag("·"))).parse(line);
    result.is_ok()
}

/// CR 714: Parse all chapter lines from a Saga's Oracle text.
/// Returns (chapter_triggers, etb_replacement, consumed_line_indices).
pub(crate) fn parse_saga_chapters(
    lines: &[&str],
    _card_name: &str,
) -> (
    Vec<TriggerDefinition>,
    ReplacementDefinition,
    HashSet<usize>,
) {
    let mut chapters: Vec<(Vec<u32>, String)> = Vec::new();
    let mut consumed = HashSet::new();

    for (idx, &line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let stripped = strip_reminder_text(trimmed);
        if stripped.is_empty() {
            continue;
        }

        if let Some((nums, effect)) = parse_chapter_line(&stripped) {
            chapters.push((nums, effect));
            consumed.insert(idx);
        } else if is_chapter_body_continuation(&stripped) && !chapters.is_empty() {
            // Multi-line chapter body: bullet-list continuation of previous chapter
            // (e.g. "I, II — Choose one —\n• Option A.\n• Option B.").
            chapters.last_mut().unwrap().1.push(' ');
            chapters.last_mut().unwrap().1.push_str(&stripped);
            consumed.insert(idx);
        }
        // Any other non-chapter line (trailing keyword like "Flying" on FIN Summon
        // sagas, or the reminder paragraph) is left for the general dispatcher.
    }

    let mut triggers = Vec::new();
    for (nums, effect_text) in &chapters {
        for &n in nums {
            let trigger = TriggerDefinition::new(TriggerMode::CounterAdded)
                .valid_card(TargetFilter::SelfRef)
                .counter_filter(CounterTriggerFilter {
                    counter_type: crate::types::counter::CounterType::Lore,
                    threshold: Some(n),
                })
                .execute(parse_effect_chain(effect_text, AbilityKind::Spell))
                .trigger_zones(vec![Zone::Battlefield])
                .description(format!("Chapter {n}"));
            triggers.push(trigger);
        }
    }

    // CR 714.3a: As a Saga enters the battlefield, its controller puts a lore counter on it.
    let etb_replacement = ReplacementDefinition::new(ReplacementEvent::Moved)
        .execute(AbilityDefinition::new(
            AbilityKind::Spell,
            Effect::PutCounter {
                counter_type: "lore".to_string(),
                count: QuantityExpr::Fixed { value: 1 },
                target: TargetFilter::SelfRef,
            },
        ))
        .valid_card(TargetFilter::SelfRef)
        .destination_zone(Zone::Battlefield)
        .description("Saga ETB lore counter".to_string());

    (triggers, etb_replacement, consumed)
}

/// Check if a line is a saga chapter (e.g. "I —", "II —", "III —").
pub(crate) fn is_saga_chapter(lower: &str) -> bool {
    parse_chapter_line(lower).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_roman_numeral_range() {
        assert_eq!(parse_roman_numeral("I"), Some(1));
        assert_eq!(parse_roman_numeral("ii"), Some(2));
        assert_eq!(parse_roman_numeral("III"), Some(3));
        assert_eq!(parse_roman_numeral("IV"), Some(4));
        assert_eq!(parse_roman_numeral("v"), Some(5));
        assert_eq!(parse_roman_numeral("VI"), Some(6));
        assert_eq!(parse_roman_numeral("VII"), Some(7));
        assert_eq!(parse_roman_numeral("VIII"), Some(8));
        assert_eq!(parse_roman_numeral("IX"), Some(9));
        assert_eq!(parse_roman_numeral("X"), Some(10));
        assert_eq!(parse_roman_numeral("XI"), Some(11));
        assert_eq!(parse_roman_numeral("XII"), Some(12));
        assert_eq!(parse_roman_numeral("XIV"), Some(14));
        assert_eq!(parse_roman_numeral("XV"), Some(15));
        assert_eq!(parse_roman_numeral("XX"), Some(20));
        // Non-roman characters return None
        assert_eq!(parse_roman_numeral("ABC"), None);
    }

    #[test]
    fn parse_chapter_line_single() {
        let (nums, effect) = parse_chapter_line("I — Draw a card.").unwrap();
        assert_eq!(nums, vec![1]);
        assert_eq!(effect, "Draw a card.");
    }

    #[test]
    fn parse_chapter_line_multi() {
        let (nums, effect) = parse_chapter_line("I, II — Target creature gets +2/+0.").unwrap();
        assert_eq!(nums, vec![1, 2]);
        assert_eq!(effect, "Target creature gets +2/+0.");
    }

    #[test]
    fn parse_chapter_line_hyphen_fallback() {
        let (nums, effect) = parse_chapter_line("III - Destroy target creature.").unwrap();
        assert_eq!(nums, vec![3]);
        assert_eq!(effect, "Destroy target creature.");
    }

    #[test]
    fn parse_chapter_line_strips_flavor_title() {
        // FIN Summon saga pattern: "I — Crescent Fang — Search your library…"
        let (nums, effect) =
            parse_chapter_line("I — Crescent Fang — Search your library for a basic land card.")
                .unwrap();
        assert_eq!(nums, vec![1]);
        assert_eq!(effect, "Search your library for a basic land card.");

        // Multi-chapter with title: "I, II — Jecht Beam — Each opponent discards a card."
        let (nums, effect) =
            parse_chapter_line("I, II — Jecht Beam — Each opponent discards a card.").unwrap();
        assert_eq!(nums, vec![1, 2]);
        assert_eq!(effect, "Each opponent discards a card.");

        // Single-word title: Weatherseed Treaty "III — Domain — Target creature…"
        let (nums, effect) =
            parse_chapter_line("III — Domain — Target creature you control gets +X/+X.").unwrap();
        assert_eq!(nums, vec![3]);
        assert_eq!(effect, "Target creature you control gets +X/+X.");

        // No title: plain chapter still works
        let (nums, effect) = parse_chapter_line("II — Create a 1/1 green Saproling.").unwrap();
        assert_eq!(nums, vec![2]);
        assert_eq!(effect, "Create a 1/1 green Saproling.");
    }

    #[test]
    fn is_saga_chapter_extended() {
        assert!(is_saga_chapter("VI — Something"));
        assert!(is_saga_chapter("VII — Something"));
        assert!(is_saga_chapter("i — something"));
        assert!(!is_saga_chapter("Draw a card."));
    }
}
