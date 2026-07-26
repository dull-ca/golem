//! The read model a poll returns (ADR 0033 §2): the attempt's per-glyph
//! progress plus its event slice, assembled from two tiers of durability. The
//! per-glyph `state` and `rounds` are folded from the attempt's `wal_step`
//! rows, which survive a restart; `events` and `next_retry_in_ms` come from the
//! in-memory ring (`progress.rs`), which does not. `project` is a pure function
//! of what it is handed — the caller (`Foreman::progress_projection`) reads both
//! tiers and passes them in.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::journal::{
    AttemptPhase, GlyphOp, ReconcileAttempt, Revision, WalAction, WalStep, WalStepState,
};
use crate::progress::ProgressEvent;
use crate::report::{
    FailClassReport, FailPhase, GlyphAction, GlyphFailure, GlyphLine, GlyphOutcome,
    ReconcileReport, UnitOutcome, UnitReport,
};

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PhaseView {
    Planning,
    Enacting,
    Settling,
    Settled,
    RolledBack,
}

// NOTE: the client vocabulary collapses the storage phases: a committed attempt
// reads as `settled`, and both the in-progress rollback and its terminal state
// read as `rolled_back` (ADR 0033 §2). `Settling` exists in the view but is
// never produced — there is no `AttemptPhase::Settling`; commit goes straight to
// `Committed`.
pub fn phase_view(phase: AttemptPhase) -> PhaseView {
    match phase {
        AttemptPhase::Planning => PhaseView::Planning,
        AttemptPhase::Enacting => PhaseView::Enacting,
        AttemptPhase::RollingBack => PhaseView::RolledBack,
        AttemptPhase::Committed => PhaseView::Settled,
        AttemptPhase::RolledBack => PhaseView::RolledBack,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GlyphState {
    Pending,
    InProgress,
    Applied,
    Unchanged,
    Failed,
    RolledBack,
}

#[derive(Debug, Clone, Serialize)]
pub struct GlyphProgress {
    pub glyph_key: String,
    pub action: String,
    pub state: GlyphState,
    pub rounds: u32,
    pub next_retry_in_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnitProgress {
    pub unit_path: Vec<String>,
    pub glyphs: Vec<GlyphProgress>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReconcileProgress {
    pub reconcile_id: u64,
    pub phase: PhaseView,
    pub units: Vec<UnitProgress>,
    pub events: Vec<ProgressEvent>,
    pub cursor: u64,
    pub report: Option<ReconcileReport>,
}

fn action_tag(op: &GlyphOp) -> &'static str {
    match op {
        GlyphOp::Install { .. } => "install",
        GlyphOp::Replace { .. } => "replace",
        GlyphOp::Remove { .. } => "remove",
        GlyphOp::Noop { .. } => "noop",
    }
}

fn fold_state(rows: &[&WalStep]) -> (GlyphState, u32) {
    let mut rounds = 0u32;
    let mut last_terminal: Option<WalStepState> = None;
    let mut saw_intended_without_terminal = false;
    let mut i = 0;
    while i < rows.len() {
        match rows[i].state {
            WalStepState::Intended => {
                let mut j = i + 1;
                let mut terminal = None;
                while j < rows.len() {
                    match rows[j].state {
                        WalStepState::Done | WalStepState::Failed | WalStepState::Reversed => {
                            terminal = Some(rows[j].state);
                            break;
                        }
                        _ => j += 1,
                    }
                }
                match terminal {
                    Some(WalStepState::Failed) => {
                        rounds += 1;
                        last_terminal = Some(WalStepState::Failed);
                        i = j + 1;
                    }
                    Some(t) => {
                        last_terminal = Some(t);
                        i = j + 1;
                    }
                    None => {
                        saw_intended_without_terminal = true;
                        i += 1;
                    }
                }
            }
            _ => i += 1,
        }
    }
    let state = if saw_intended_without_terminal {
        GlyphState::InProgress
    } else {
        match last_terminal {
            Some(WalStepState::Done) => GlyphState::Applied,
            Some(WalStepState::Failed) => GlyphState::Failed,
            Some(WalStepState::Reversed) => GlyphState::RolledBack,
            _ => GlyphState::Pending,
        }
    };
    (
        state,
        rounds.max(if matches!(state, GlyphState::Failed) {
            1
        } else {
            0
        }),
    )
}

/// One unit's rows grouped for folding: its `unit_path` and, per glyph key in
/// first-appearance order, that glyph's `wal_step` rows in seq order.
type GroupedUnit<'a> = (Vec<String>, Vec<(String, Vec<&'a WalStep>)>);

/// Group one attempt's non-`Restart` `wal_step` rows into ordered units, each an
/// ordered list of `(glyph_key, its rows in seq order)`. First-appearance order
/// is preserved for both units and keys so the projection and the rebuilt report
/// present glyphs in enact order. Shared by [`project`] and [`rebuild_report`] so
/// the live view and the reattach-rebuilt report fold the same rows the same way.
fn group_units<'a>(attempt: &ReconcileAttempt, steps: &'a [WalStep]) -> Vec<GroupedUnit<'a>> {
    let mut order: Vec<Vec<String>> = Vec::new();
    let mut by_unit: BTreeMap<Vec<String>, Vec<String>> = BTreeMap::new();
    let mut rows_by_key: BTreeMap<(Vec<String>, String), Vec<&WalStep>> = BTreeMap::new();
    for step in steps
        .iter()
        .filter(|s| s.reconcile_id == attempt.reconcile_id)
    {
        if step.action == WalAction::Restart {
            continue;
        }
        let unit = step.unit_path.clone();
        if !order.contains(&unit) {
            order.push(unit.clone());
        }
        let keys = by_unit.entry(unit.clone()).or_default();
        if !keys.contains(&step.glyph_key) {
            keys.push(step.glyph_key.clone());
        }
        rows_by_key
            .entry((unit, step.glyph_key.clone()))
            .or_default()
            .push(step);
    }
    order
        .into_iter()
        .map(|unit| {
            let glyphs = by_unit[&unit]
                .iter()
                .map(|key| {
                    let rows = rows_by_key[&(unit.clone(), key.clone())].clone();
                    (key.clone(), rows)
                })
                .collect();
            (unit, glyphs)
        })
        .collect()
}

pub fn project(
    attempt: &ReconcileAttempt,
    steps: &[WalStep],
    events: Vec<ProgressEvent>,
    report: Option<ReconcileReport>,
    retries: &BTreeMap<String, u64>,
) -> ReconcileProgress {
    let mut units = Vec::new();
    for (unit, keyed) in group_units(attempt, steps) {
        let mut glyphs = Vec::new();
        for (key, rows) in &keyed {
            let (state, rounds) = fold_state(rows);
            let action = rows
                .last()
                .map(|s| action_tag(&s.op).to_string())
                .unwrap_or_else(|| "install".into());
            let state = if action == "noop" && matches!(state, GlyphState::Applied) {
                GlyphState::Unchanged
            } else {
                state
            };
            glyphs.push(GlyphProgress {
                glyph_key: key.clone(),
                action,
                state,
                rounds,
                next_retry_in_ms: retries.get(key).copied(),
            });
        }
        units.push(UnitProgress {
            unit_path: unit.clone(),
            glyphs,
        });
    }
    let cursor = events.iter().map(|e| e.seq).max().unwrap_or(0);
    ReconcileProgress {
        reconcile_id: attempt.reconcile_id,
        phase: phase_view(attempt.phase),
        units,
        events,
        cursor,
        report,
    }
}

/// Reconstruct a settled attempt's [`ReconcileReport`] from durable state alone —
/// its `wal_step` rows folded per glyph, plus the `revision` the caller resolved
/// (`wal::revision_for_attempt`). This is the reattach path (ADR 0033 §2): after a
/// daemon restart the in-memory report cache is empty, but a poll of an
/// already-settled id must still carry a report, so it is rebuilt here rather than
/// omitted. What the WAL cannot reproduce is degraded honestly, not dropped: a
/// failure's `message` is empty, its `details` forensics are `None`, and its
/// `attempts` is the WAL-derived round count (the exact fatal-vs-exhausted class
/// and the reconciler's reason string lived only in the lost in-memory round
/// state). The per-glyph outcomes, unit paths, and the revision are exact.
pub fn rebuild_report(
    attempt: &ReconcileAttempt,
    steps: &[WalStep],
    revision: Revision,
) -> ReconcileReport {
    let mut units = Vec::new();
    for (unit_path, keyed) in group_units(attempt, steps) {
        let mut glyphs = Vec::new();
        let mut failures = Vec::new();
        for (key, rows) in &keyed {
            let (state, rounds) = fold_state(rows);
            let action = rows
                .last()
                .map(|s| glyph_action(&s.op))
                .unwrap_or(GlyphAction::Install);
            let (state, action) =
                if matches!(action, GlyphAction::Noop) && matches!(state, GlyphState::Applied) {
                    (GlyphState::Unchanged, action)
                } else {
                    (state, action)
                };
            let outcome = match state {
                GlyphState::Applied | GlyphState::Pending | GlyphState::InProgress => {
                    GlyphOutcome::Applied
                }
                GlyphState::Unchanged => GlyphOutcome::Unchanged,
                GlyphState::Failed => GlyphOutcome::Failed,
                GlyphState::RolledBack => GlyphOutcome::RolledBack,
            };
            let attempts = match outcome {
                GlyphOutcome::Failed => rounds.max(1),
                GlyphOutcome::Unchanged => 0,
                _ => 1,
            };
            glyphs.push(GlyphLine {
                glyph_key: key.clone(),
                action,
                outcome,
                attempts,
                message: None,
            });
            if matches!(state, GlyphState::Failed) {
                failures.push(GlyphFailure {
                    glyph_key: key.clone(),
                    unit_path: unit_path.clone(),
                    phase: FailPhase::Enact,
                    class: FailClassReport::RetriesExhausted,
                    attempts: rounds.max(1),
                    message: String::new(),
                    rolled_back: false,
                    details: None,
                });
            }
        }
        let any_rolled_back = glyphs
            .iter()
            .any(|g| matches!(g.outcome, GlyphOutcome::RolledBack));
        let outcome = if failures.is_empty() {
            UnitOutcome::Settled
        } else if any_rolled_back {
            UnitOutcome::RolledBack
        } else {
            UnitOutcome::Partial
        };
        for f in &mut failures {
            f.rolled_back = any_rolled_back;
        }
        units.push(UnitReport {
            unit_path,
            outcome,
            glyphs,
            failures,
        });
    }
    ReconcileReport::roll_up(revision, units)
}

fn glyph_action(op: &GlyphOp) -> GlyphAction {
    match op {
        GlyphOp::Install { .. } => GlyphAction::Install,
        GlyphOp::Replace { .. } => GlyphAction::Replace,
        GlyphOp::Remove { .. } => GlyphAction::Remove,
        GlyphOp::Noop { .. } => GlyphAction::Noop,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::{GlyphOp, ReconcileAttempt, WalAction, WalStep, WalStepState};
    use chrono::Utc;
    use scroll_format::Glyph;
    use std::collections::BTreeMap;

    fn apt_op(name: &str) -> GlyphOp {
        let glyph = Glyph::AptPackage { name: name.into() };
        GlyphOp::Install {
            cid: scroll_format::content_id_of_glyph(&glyph),
            glyph,
        }
    }

    fn step(seq: u64, ord: u64, key: &str, state: WalStepState, unit: &[&str]) -> WalStep {
        WalStep {
            seq,
            reconcile_id: 1,
            step_ord: ord,
            glyph_key: key.into(),
            action: WalAction::Apply,
            state,
            op: apt_op(key.trim_start_matches("apt:")),
            inverse: None,
            changed: None,
            unit_path: unit.iter().map(|s| s.to_string()).collect(),
            at: Utc::now(),
        }
    }

    fn attempt(phase: AttemptPhase) -> ReconcileAttempt {
        ReconcileAttempt {
            reconcile_id: 1,
            started_at: Utc::now(),
            scroll_content_id: None,
            phase,
            settled_at: None,
        }
    }

    #[test]
    fn a_done_glyph_projects_applied_and_a_bare_intended_is_in_progress() {
        let steps = vec![
            step(1, 0, "apt:nginx", WalStepState::Intended, &["scaly", "a"]),
            step(2, 0, "apt:nginx", WalStepState::Done, &["scaly", "a"]),
            step(3, 1, "apt:pg", WalStepState::Intended, &["scaly", "a"]),
        ];
        let p = project(
            &attempt(AttemptPhase::Enacting),
            &steps,
            vec![],
            None,
            &BTreeMap::new(),
        );
        assert!(matches!(p.phase, PhaseView::Enacting));
        assert_eq!(p.units.len(), 1);
        assert_eq!(p.units[0].unit_path, vec!["scaly", "a"]);
        let nginx = p.units[0]
            .glyphs
            .iter()
            .find(|g| g.glyph_key == "apt:nginx")
            .unwrap();
        assert!(matches!(nginx.state, GlyphState::Applied));
        let pg = p.units[0]
            .glyphs
            .iter()
            .find(|g| g.glyph_key == "apt:pg")
            .unwrap();
        assert!(matches!(pg.state, GlyphState::InProgress));
        assert!(p.report.is_none());
    }

    #[test]
    fn committed_phase_serializes_as_settled() {
        let v = serde_json::to_value(phase_view(AttemptPhase::Committed)).unwrap();
        assert_eq!(v, "settled");
    }

    #[test]
    fn repeated_intended_failed_brackets_count_rounds() {
        let steps = vec![
            step(1, 0, "apt:x", WalStepState::Intended, &["scaly"]),
            step(2, 0, "apt:x", WalStepState::Failed, &["scaly"]),
            step(3, 0, "apt:x", WalStepState::Intended, &["scaly"]),
            step(4, 0, "apt:x", WalStepState::Failed, &["scaly"]),
        ];
        let p = project(
            &attempt(AttemptPhase::Enacting),
            &steps,
            vec![],
            None,
            &BTreeMap::new(),
        );
        let g = &p.units[0].glyphs[0];
        assert!(matches!(g.state, GlyphState::Failed));
        assert_eq!(g.rounds, 2);
    }
}
