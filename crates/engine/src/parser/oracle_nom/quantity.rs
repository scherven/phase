//! Quantity expression combinators for Oracle text parsing.
//!
//! Parses quantity expressions from Oracle text: fixed numbers, dynamic references
//! like "the number of creatures you control", "its power", "your life total", etc.

use nom::branch::alt;
use nom::bytes::complete::tag;
use nom::combinator::{map, value};
use nom::Parser;

use super::error::OracleResult;
use super::primitives::parse_number;
use crate::types::ability::{QuantityExpr, QuantityRef};

/// Parse a quantity expression: either a fixed number or a dynamic reference.
pub fn parse_quantity(input: &str) -> OracleResult<'_, QuantityExpr> {
    alt((
        map(parse_quantity_ref, |qty| QuantityExpr::Ref { qty }),
        map(parse_number, |n| QuantityExpr::Fixed { value: n as i32 }),
    ))
    .parse(input)
}

/// Parse a dynamic quantity reference from Oracle text.
///
/// Matches phrases like "the number of creatures you control", "its power",
/// "your life total", "cards in your hand", etc.
pub fn parse_quantity_ref(input: &str) -> OracleResult<'_, QuantityRef> {
    alt((
        parse_life_total_ref,
        parse_hand_size_ref,
        parse_graveyard_size_ref,
        parse_self_power_ref,
        parse_self_toughness_ref,
        parse_life_lost_ref,
        parse_life_gained_ref,
        parse_starting_life_ref,
    ))
    .or(alt((parse_speed_ref,)))
    .parse(input)
}

/// Parse "your life total".
fn parse_life_total_ref(input: &str) -> OracleResult<'_, QuantityRef> {
    value(QuantityRef::LifeTotal, tag("your life total")).parse(input)
}

/// Parse "cards in your hand".
fn parse_hand_size_ref(input: &str) -> OracleResult<'_, QuantityRef> {
    value(QuantityRef::HandSize, tag("cards in your hand")).parse(input)
}

/// Parse "cards in your graveyard".
fn parse_graveyard_size_ref(input: &str) -> OracleResult<'_, QuantityRef> {
    value(QuantityRef::GraveyardSize, tag("cards in your graveyard")).parse(input)
}

/// Parse "its power" / "~'s power" / "this creature's power".
fn parse_self_power_ref(input: &str) -> OracleResult<'_, QuantityRef> {
    alt((
        value(QuantityRef::SelfPower, tag("its power")),
        value(QuantityRef::SelfPower, tag("~'s power")),
        value(QuantityRef::SelfPower, tag("this creature's power")),
    ))
    .parse(input)
}

/// Parse "its toughness" / "~'s toughness" / "this creature's toughness".
fn parse_self_toughness_ref(input: &str) -> OracleResult<'_, QuantityRef> {
    alt((
        value(QuantityRef::SelfToughness, tag("its toughness")),
        value(QuantityRef::SelfToughness, tag("~'s toughness")),
        value(QuantityRef::SelfToughness, tag("this creature's toughness")),
    ))
    .parse(input)
}

/// Parse life-lost references: "the life you've lost this turn", "life you've lost", etc.
/// Includes duration-stripped forms (without "this turn") for post-duration-stripping contexts.
fn parse_life_lost_ref(input: &str) -> OracleResult<'_, QuantityRef> {
    alt((
        value(
            QuantityRef::LifeLostThisTurn,
            tag("total life you lost this turn"),
        ),
        value(
            QuantityRef::LifeLostThisTurn,
            tag("total life you've lost this turn"),
        ),
        value(
            QuantityRef::LifeLostThisTurn,
            tag("the life you've lost this turn"),
        ),
        value(
            QuantityRef::LifeLostThisTurn,
            tag("the life you lost this turn"),
        ),
        value(
            QuantityRef::LifeLostThisTurn,
            tag("life you've lost this turn"),
        ),
        value(
            QuantityRef::LifeLostThisTurn,
            tag("life you lost this turn"),
        ),
        // Duration-stripped forms (after strip_trailing_duration removes "this turn")
        value(QuantityRef::LifeLostThisTurn, tag("the life you've lost")),
        value(QuantityRef::LifeLostThisTurn, tag("the life you lost")),
        value(QuantityRef::LifeLostThisTurn, tag("life you've lost")),
        value(QuantityRef::LifeLostThisTurn, tag("life you lost")),
    ))
    .parse(input)
}

/// Parse life-gained references: "the life you've gained this turn", "life you've gained", etc.
/// Includes duration-stripped forms (without "this turn") for post-duration-stripping contexts.
fn parse_life_gained_ref(input: &str) -> OracleResult<'_, QuantityRef> {
    alt((
        value(
            QuantityRef::LifeGainedThisTurn,
            tag("total life you gained this turn"),
        ),
        value(
            QuantityRef::LifeGainedThisTurn,
            tag("total life you've gained this turn"),
        ),
        value(
            QuantityRef::LifeGainedThisTurn,
            tag("the life you've gained this turn"),
        ),
        value(
            QuantityRef::LifeGainedThisTurn,
            tag("the life you gained this turn"),
        ),
        value(
            QuantityRef::LifeGainedThisTurn,
            tag("life you've gained this turn"),
        ),
        value(
            QuantityRef::LifeGainedThisTurn,
            tag("life you gained this turn"),
        ),
        // Duration-stripped forms
        value(
            QuantityRef::LifeGainedThisTurn,
            tag("the life you've gained"),
        ),
        value(QuantityRef::LifeGainedThisTurn, tag("the life you gained")),
        value(QuantityRef::LifeGainedThisTurn, tag("life you've gained")),
        value(QuantityRef::LifeGainedThisTurn, tag("life you gained")),
    ))
    .parse(input)
}

/// Parse "your starting life total".
fn parse_starting_life_ref(input: &str) -> OracleResult<'_, QuantityRef> {
    value(
        QuantityRef::StartingLifeTotal,
        tag("your starting life total"),
    )
    .parse(input)
}

/// Parse "your speed".
fn parse_speed_ref(input: &str) -> OracleResult<'_, QuantityRef> {
    value(QuantityRef::Speed, tag("your speed")).parse(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_quantity_fixed() {
        let (rest, q) = parse_quantity("3 damage").unwrap();
        assert_eq!(q, QuantityExpr::Fixed { value: 3 });
        assert_eq!(rest, " damage");
    }

    #[test]
    fn test_parse_quantity_ref_life_total() {
        let (rest, q) = parse_quantity("your life total").unwrap();
        assert_eq!(
            q,
            QuantityExpr::Ref {
                qty: QuantityRef::LifeTotal
            }
        );
        assert_eq!(rest, "");
    }

    #[test]
    fn test_parse_quantity_ref_hand_size() {
        let (rest, q) = parse_quantity_ref("cards in your hand").unwrap();
        assert_eq!(q, QuantityRef::HandSize);
        assert_eq!(rest, "");
    }

    #[test]
    fn test_parse_quantity_ref_self_power() {
        let (rest, q) = parse_quantity_ref("its power").unwrap();
        assert_eq!(q, QuantityRef::SelfPower);
        assert_eq!(rest, "");
    }

    #[test]
    fn test_parse_quantity_ref_graveyard() {
        let (rest, q) = parse_quantity_ref("cards in your graveyard and").unwrap();
        assert_eq!(q, QuantityRef::GraveyardSize);
        assert_eq!(rest, " and");
    }

    #[test]
    fn test_parse_quantity_ref_life_lost() {
        let (rest, q) = parse_quantity_ref("the life you've lost this turn").unwrap();
        assert_eq!(q, QuantityRef::LifeLostThisTurn);
        assert_eq!(rest, "");
    }

    #[test]
    fn test_parse_quantity_failure() {
        assert!(parse_quantity("xyz").is_err());
    }
}
