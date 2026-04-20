use serde::{Deserialize, Serialize};

/// Counter types serialize as flat strings so they can be used as JSON map keys
/// in `HashMap<CounterType, u32>`. Without this, `Generic("quest")` would serialize
/// as `{"Generic":"quest"}` which serde_json rejects as a map key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CounterType {
    Plus1Plus1,
    Minus1Minus1,
    Loyalty,
    /// CR 122.1g + CR 310.4: The number of defense counters on a battle on the
    /// battlefield indicates its defense. A battle with 0 defense is put into
    /// its owner's graveyard as a state-based action (CR 704.5v).
    Defense,
    /// CR 122.1g: When a permanent with a stun counter would become untapped during its
    /// controller's untap step, one stun counter is removed instead of untapping.
    Stun,
    /// CR 714.1: Lore counters track Saga chapter progression.
    Lore,
    Generic(String),
}

impl CounterType {
    pub fn as_str(&self) -> &str {
        match self {
            CounterType::Plus1Plus1 => "P1P1",
            CounterType::Minus1Minus1 => "M1M1",
            CounterType::Loyalty => "loyalty",
            CounterType::Defense => "defense",
            CounterType::Stun => "stun",
            CounterType::Lore => "lore",
            CounterType::Generic(s) => s.as_str(),
        }
    }
}

impl serde::Serialize for CounterType {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> serde::Deserialize<'de> for CounterType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(parse_counter_type(&s))
    }
}

/// Which counter(s) a predicate is matching against.
///
/// CR 122.1: "A counter is a marker placed on an object or player…" — some
/// Oracle text distinguishes counters by type ("a +1/+1 counter"), while
/// other text refers to counters generically ("a counter on it", meaning
/// any type). `CounterMatch::Any` captures the latter case so predicates
/// can sum across every counter type on an object, and `OfType` captures
/// the former by reusing the canonical `CounterType` enum. Prefer this over
/// `Option<CounterType>`: "Any" is a first-class matching mode rather than
/// an absence-of-specification.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum CounterMatch {
    /// "a counter on it" — any counter type; predicates sum across all types.
    Any,
    /// A specific counter type, matching the canonical `CounterType` enum.
    OfType(CounterType),
}

pub fn parse_counter_type(text: &str) -> CounterType {
    match text.trim().trim_end_matches(" counter").trim() {
        "P1P1" | "+1/+1" | "plus1plus1" => CounterType::Plus1Plus1,
        "M1M1" | "-1/-1" | "minus1minus1" => CounterType::Minus1Minus1,
        "LOYALTY" | "loyalty" => CounterType::Loyalty,
        "defense" | "DEFENSE" => CounterType::Defense,
        "stun" => CounterType::Stun,
        "lore" | "LORE" => CounterType::Lore,
        other => CounterType::Generic(other.to_string()),
    }
}
