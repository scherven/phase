// allow: no_name_matching_self
//! Architectural lint: feature modules must classify cards structurally —
//! over `CardFace` triggers, effects, and filters — never by literal name.
//!
//! Greps every `.rs` file under `crates/phase-ai/src/features/` for the
//! anti-patterns documented in the design plan. Files containing the marker
//! `allow: no_name_matching_self` (used by this lint module to talk about the
//! patterns it detects) are exempted.

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
fn feature_modules_have_no_card_name_matching() {
    let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src/features"));
    let mut violations: Vec<String> = Vec::new();
    walk(root, &mut violations);
    assert!(
        violations.is_empty(),
        "Feature modules contain card-name matching anti-patterns:\n{}",
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
