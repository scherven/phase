//! Landfall feature stub.
//!
//! Phase A scope: this module exists only as a placeholder so the
//! `features/tests/no_name_matching.rs` lint has a real directory to scan.
//! Detection logic (`detect`, payoff/enabler counting) lands in Phase B.
//!
//! Parser AST verification (Phase A task 1) — VERIFIED:
//! - `TriggerMode::ChangesZone` captures land-ETB events
//!   (see `crates/engine/src/types/triggers.rs:24-27`, CR 603.6a).
//! - `ControllerRef::You` vs `ControllerRef::Opponent` distinguish controller
//!   in trigger filters (see `crates/engine/src/types/ability.rs:813-818`).
//! - `Zone::Battlefield` is recoverable from `TriggerDefinition.destination`
//!   (see `crates/engine/src/types/ability.rs:4443`).
//!
//! No parser remediation required — landfall-shaped triggers can be
//! structurally classified using existing typed AST in Phase B.

/// Placeholder for the per-deck landfall feature. Populated in Phase B.
#[derive(Debug, Clone, Default)]
pub struct LandfallFeature {
    pub payoff_count: u32,
    pub enabler_count: u32,
    pub commitment: f32,
    pub payoff_names: Vec<String>,
}
