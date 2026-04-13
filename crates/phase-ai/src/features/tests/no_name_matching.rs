// allow: no_name_matching_self
//! Architectural lint: feature modules AND policy modules must classify
//! cards structurally — over `CardFace` triggers, effects, and filters —
//! never by literal name.
//!
//! Greps every `.rs` file under `crates/phase-ai/src/features/` and
//! `crates/phase-ai/src/policies/` for the anti-patterns listed in
//! `ANTI_PATTERNS`. Files containing the marker `allow: no_name_matching_self`
//! (used by this lint module to talk about the patterns it detects) are
//! exempted.
//!
//! ## What is explicitly allowed
//!
//! The `ANTI_PATTERNS` list intentionally does NOT catch
//! `payoff_names.contains(&obj.name)` or similar identity-lookup patterns.
//! Those are legitimate uses of a feature's pre-computed name set for
//! runtime battlefield/hand identity checks (e.g., "is a landfall payoff
//! currently on my battlefield?"). Classification — deciding whether a
//! given card is a landfall payoff — must be done structurally at feature
//! detection time and never by name. The distinction is:
//!
//! - **Forbidden** (classification by name): `obj.name == "Omnath"`.
//! - **Allowed** (identity lookup of already-classified cards):
//!   `features.landfall.payoff_names.contains(&obj.name)`.

use std::fs;
use std::path::Path;

const ANTI_PATTERNS: &[&str] = &[
    "obj.name ==",
    ".name.contains(",
    "card.name ==",
    "card.name.eq",
    "match card.name.as_str()",
];

const ALLOW_MARKER: &str = "allow: no_name_matching_self";

#[test]
fn feature_and_policy_modules_have_no_card_name_matching() {
    let roots = [
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src/features")),
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src/policies")),
    ];
    let mut violations: Vec<String> = Vec::new();
    for root in roots {
        walk(root, &mut violations);
    }
    assert!(
        violations.is_empty(),
        "Feature/policy modules contain card-name matching anti-patterns:\n{}",
        violations.join("\n")
    );
}

fn walk(dir: &Path, violations: &mut Vec<String>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, violations);
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let Ok(contents) = fs::read_to_string(&path) else {
            continue;
        };
        if contents.contains(ALLOW_MARKER) {
            continue;
        }
        for pattern in ANTI_PATTERNS {
            if contents.contains(pattern) {
                violations.push(format!("{}: contains `{}`", path.display(), pattern));
            }
        }
    }
}
