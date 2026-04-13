//! Baseline-vs-current comparison of two `SuiteReport` JSON files.
//!
//! Emits a markdown table and returns a `CompareReport` whose `any_fail()`
//! determines the process exit code. This is the CI gate for the duel suite:
//! regressions (PASS→FAIL, winrate drift > fail threshold, new matchups that
//! are already failing) return a non-zero status.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use super::run::{MatchupResult, SuiteReport, SuiteStatus};
use super::FeatureKind;

/// Drift thresholds in percentage points (0..100 scale). A drift of +10.0pp
/// means P0 winrate rose by 10 percentage points vs baseline.
#[derive(Debug, Clone, Copy)]
pub struct CompareOptions {
    pub warn_pp: f32,
    pub fail_pp: f32,
}

impl Default for CompareOptions {
    fn default() -> Self {
        Self {
            warn_pp: 8.0,
            fail_pp: 15.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareStatus {
    Pass,
    Warn,
    Fail,
    New,
    Removed,
}

#[derive(Debug, Clone)]
pub struct CompareRow {
    pub matchup_id: String,
    pub exercises: Vec<FeatureKind>,
    pub baseline: Option<MatchupResult>,
    pub current: Option<MatchupResult>,
    pub delta_p0_pp: Option<f32>,
    pub status: CompareStatus,
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CompareReport {
    pub rows: Vec<CompareRow>,
}

impl CompareReport {
    /// True if any row regressed (PASS→FAIL, drift beyond fail threshold, or
    /// new matchup that is already failing). Drives the compare exit code.
    pub fn any_fail(&self) -> bool {
        self.rows
            .iter()
            .any(|r| matches!(r.status, CompareStatus::Fail))
    }
}

#[derive(Debug)]
pub enum CompareError {
    Io(std::io::Error),
    Parse(serde_json::Error),
    SchemaMismatch { baseline: u32, current: u32 },
}

impl std::fmt::Display for CompareError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompareError::Io(e) => write!(f, "compare I/O error: {e}"),
            CompareError::Parse(e) => write!(f, "compare parse error: {e}"),
            CompareError::SchemaMismatch { baseline, current } => write!(
                f,
                "schema_version mismatch: baseline={baseline} current={current}"
            ),
        }
    }
}

impl std::error::Error for CompareError {}

impl From<std::io::Error> for CompareError {
    fn from(e: std::io::Error) -> Self {
        CompareError::Io(e)
    }
}

impl From<serde_json::Error> for CompareError {
    fn from(e: serde_json::Error) -> Self {
        CompareError::Parse(e)
    }
}

/// Read a `SuiteReport` from a JSON file.
pub fn load_report(path: &Path) -> Result<SuiteReport, CompareError> {
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    let report: SuiteReport = serde_json::from_reader(reader)?;
    Ok(report)
}

/// Core comparison entry point. Takes two reports and an options block;
/// returns a `CompareReport` whose `any_fail()` drives the exit code.
pub fn compare(
    baseline: &SuiteReport,
    current: &SuiteReport,
    options: &CompareOptions,
) -> Result<CompareReport, CompareError> {
    if baseline.schema_version != current.schema_version {
        return Err(CompareError::SchemaMismatch {
            baseline: baseline.schema_version,
            current: current.schema_version,
        });
    }

    // BTreeMap for deterministic iteration order.
    let baseline_by_id: BTreeMap<&str, &MatchupResult> = baseline
        .results
        .iter()
        .map(|r| (r.matchup_id.as_str(), r))
        .collect();
    let current_by_id: BTreeMap<&str, &MatchupResult> = current
        .results
        .iter()
        .map(|r| (r.matchup_id.as_str(), r))
        .collect();

    let mut ids: HashSet<&str> = HashSet::new();
    ids.extend(baseline_by_id.keys().copied());
    ids.extend(current_by_id.keys().copied());
    let mut ids: Vec<&str> = ids.into_iter().collect();
    ids.sort();

    let mut rows = Vec::with_capacity(ids.len());
    for id in ids {
        let baseline_row = baseline_by_id.get(id).copied();
        let current_row = current_by_id.get(id).copied();
        rows.push(classify_row(id, baseline_row, current_row, options));
    }

    Ok(CompareReport { rows })
}

fn classify_row(
    id: &str,
    baseline: Option<&MatchupResult>,
    current: Option<&MatchupResult>,
    options: &CompareOptions,
) -> CompareRow {
    match (baseline, current) {
        (None, None) => unreachable!("id must appear in at least one report"),
        (Some(b), None) => CompareRow {
            matchup_id: id.to_string(),
            exercises: b.exercises.clone(),
            baseline: Some(b.clone()),
            current: None,
            delta_p0_pp: None,
            status: CompareStatus::Removed,
            reason: Some("matchup removed from current report".to_string()),
        },
        (None, Some(c)) => {
            let (status, reason) = match c.status {
                SuiteStatus::Fail => (
                    CompareStatus::Fail,
                    Some(format!(
                        "new matchup is already failing: {}",
                        c.fail_reason.as_deref().unwrap_or("no reason")
                    )),
                ),
                _ => (CompareStatus::New, Some("matchup is new".to_string())),
            };
            CompareRow {
                matchup_id: id.to_string(),
                exercises: c.exercises.clone(),
                baseline: None,
                current: Some(c.clone()),
                delta_p0_pp: None,
                status,
                reason,
            }
        }
        (Some(b), Some(c)) => {
            let b_rate = winrate(b);
            let c_rate = winrate(c);
            let delta_pp = (c_rate - b_rate) * 100.0;
            let regressed_status =
                matches!(b.status, SuiteStatus::Pass) && matches!(c.status, SuiteStatus::Fail);

            let (status, reason) = if regressed_status {
                (
                    CompareStatus::Fail,
                    Some(format!(
                        "regression: baseline PASS → current FAIL ({})",
                        c.fail_reason.as_deref().unwrap_or("no reason")
                    )),
                )
            } else if delta_pp.abs() > options.fail_pp {
                (
                    CompareStatus::Fail,
                    Some(format!(
                        "winrate drift {:+.1}pp exceeds fail threshold ±{:.1}pp",
                        delta_pp, options.fail_pp
                    )),
                )
            } else if delta_pp.abs() > options.warn_pp {
                (
                    CompareStatus::Warn,
                    Some(format!("winrate drift {delta_pp:+.1}pp")),
                )
            } else {
                (CompareStatus::Pass, None)
            };

            CompareRow {
                matchup_id: id.to_string(),
                exercises: c.exercises.clone(),
                baseline: Some(b.clone()),
                current: Some(c.clone()),
                delta_p0_pp: Some(delta_pp),
                status,
                reason,
            }
        }
    }
}

fn winrate(r: &MatchupResult) -> f32 {
    let total = r.p0_wins + r.p1_wins + r.draws;
    if total == 0 {
        0.0
    } else {
        r.p0_wins as f32 / total as f32
    }
}

fn status_str(s: CompareStatus) -> &'static str {
    match s {
        CompareStatus::Pass => "PASS",
        CompareStatus::Warn => "WARN",
        CompareStatus::Fail => "FAIL",
        CompareStatus::New => "NEW",
        CompareStatus::Removed => "REMOVED",
    }
}

/// Render a markdown table of the comparison to stdout + emit a summary line.
pub fn print_markdown(report: &CompareReport) {
    println!();
    println!("| matchup | exercises | baseline p0% | current p0% | Δpp | status |");
    println!("|---------|-----------|--------------|-------------|-----|--------|");
    for row in &report.rows {
        let exercises: Vec<String> = row.exercises.iter().map(|f| format!("{f:?}")).collect();
        let baseline_cell = match &row.baseline {
            Some(b) => format!("{:.0}%", winrate(b) * 100.0),
            None => "—".to_string(),
        };
        let current_cell = match &row.current {
            Some(c) => format!("{:.0}%", winrate(c) * 100.0),
            None => "—".to_string(),
        };
        let delta_cell = match row.delta_p0_pp {
            Some(d) => format!("{d:+.1}pp"),
            None => "—".to_string(),
        };
        println!(
            "| {} | {} | {} | {} | {} | {} |",
            row.matchup_id,
            exercises.join(", "),
            baseline_cell,
            current_cell,
            delta_cell,
            status_str(row.status),
        );
        if let Some(reason) = &row.reason {
            if !matches!(row.status, CompareStatus::Pass) {
                println!("|  ↳ _{reason}_ | | | | | |");
            }
        }
    }

    let mut pass = 0usize;
    let mut warn = 0usize;
    let mut fail = 0usize;
    let mut new = 0usize;
    let mut removed = 0usize;
    for row in &report.rows {
        match row.status {
            CompareStatus::Pass => pass += 1,
            CompareStatus::Warn => warn += 1,
            CompareStatus::Fail => fail += 1,
            CompareStatus::New => new += 1,
            CompareStatus::Removed => removed += 1,
        }
    }
    println!("\ncompare: {fail} FAIL, {warn} WARN, {pass} PASS, {new} NEW, {removed} REMOVED");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::duel_suite::run::{MatchupResult, SuiteReport, SuiteStatus};
    use crate::duel_suite::{Expected, FeatureKind};

    fn mk_report(results: Vec<MatchupResult>) -> SuiteReport {
        SuiteReport {
            schema_version: 1,
            unix_timestamp_secs: 0,
            difficulty: "Easy".into(),
            games_per_matchup: 10,
            base_seed: 0,
            results,
        }
    }

    fn mk_result(id: &str, p0_wins: usize, total: usize, status: SuiteStatus) -> MatchupResult {
        let total = total.max(p0_wins);
        let p1_wins = total - p0_wins;
        MatchupResult {
            matchup_id: id.into(),
            exercises: vec![FeatureKind::AggroPressure],
            p0_label: "A".into(),
            p1_label: "B".into(),
            expected: Expected::Mirror { tolerance: 0.15 },
            p0_wins,
            p1_wins,
            draws: 0,
            total_turns: 0,
            total_duration_ms: 0,
            avg_turns: 10.0,
            avg_duration_ms: 1000.0,
            status,
            fail_reason: if matches!(status, SuiteStatus::Fail) {
                Some("mock fail".into())
            } else {
                None
            },
            attribution: None,
        }
    }

    #[test]
    fn compare_identity_is_pass() {
        let report = mk_report(vec![mk_result("red-mirror", 5, 10, SuiteStatus::Pass)]);
        let result = compare(&report, &report, &CompareOptions::default()).unwrap();
        assert!(!result.any_fail());
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].status, CompareStatus::Pass);
    }

    #[test]
    fn compare_regression_pass_to_fail_flags_fail() {
        let baseline = mk_report(vec![mk_result("red-mirror", 5, 10, SuiteStatus::Pass)]);
        let current = mk_report(vec![mk_result("red-mirror", 1, 10, SuiteStatus::Fail)]);
        let result = compare(&baseline, &current, &CompareOptions::default()).unwrap();
        assert!(result.any_fail());
        assert_eq!(result.rows[0].status, CompareStatus::Fail);
        assert!(result.rows[0]
            .reason
            .as_ref()
            .unwrap()
            .contains("regression"));
    }

    #[test]
    fn compare_drift_over_fail_threshold_flags_fail() {
        let baseline = mk_report(vec![mk_result("m", 5, 10, SuiteStatus::Pass)]);
        let current = mk_report(vec![mk_result("m", 8, 10, SuiteStatus::Pass)]); // 30pp drift
        let result = compare(&baseline, &current, &CompareOptions::default()).unwrap();
        assert!(result.any_fail());
        assert_eq!(result.rows[0].status, CompareStatus::Fail);
    }

    #[test]
    fn compare_drift_between_warn_and_fail_flags_warn() {
        // Default warn=8, fail=15. Craft a 10pp drift so we land in warn.
        let baseline = mk_report(vec![mk_result("m", 50, 100, SuiteStatus::Pass)]);
        let current = mk_report(vec![mk_result("m", 60, 100, SuiteStatus::Pass)]);
        let result = compare(&baseline, &current, &CompareOptions::default()).unwrap();
        assert!(!result.any_fail());
        assert_eq!(result.rows[0].status, CompareStatus::Warn);
    }

    #[test]
    fn compare_new_matchup_flagged_as_new() {
        let baseline = mk_report(vec![]);
        let current = mk_report(vec![mk_result("x", 5, 10, SuiteStatus::Pass)]);
        let result = compare(&baseline, &current, &CompareOptions::default()).unwrap();
        assert_eq!(result.rows[0].status, CompareStatus::New);
        assert!(!result.any_fail());
    }

    #[test]
    fn compare_new_failing_matchup_flagged_as_fail() {
        let baseline = mk_report(vec![]);
        let current = mk_report(vec![mk_result("x", 0, 10, SuiteStatus::Fail)]);
        let result = compare(&baseline, &current, &CompareOptions::default()).unwrap();
        assert_eq!(result.rows[0].status, CompareStatus::Fail);
        assert!(result.any_fail());
    }

    #[test]
    fn compare_removed_matchup_is_informational() {
        let baseline = mk_report(vec![mk_result("gone", 5, 10, SuiteStatus::Pass)]);
        let current = mk_report(vec![]);
        let result = compare(&baseline, &current, &CompareOptions::default()).unwrap();
        assert_eq!(result.rows[0].status, CompareStatus::Removed);
        assert!(!result.any_fail());
    }

    #[test]
    fn compare_schema_mismatch_returns_error() {
        let mut baseline = mk_report(vec![]);
        baseline.schema_version = 2;
        let current = mk_report(vec![]);
        let err = compare(&baseline, &current, &CompareOptions::default()).unwrap_err();
        assert!(matches!(err, CompareError::SchemaMismatch { .. }));
    }
}
