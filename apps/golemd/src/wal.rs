//! The fold from the write-ahead log to the currently-applied set (ADR 0020 §1).
//! The applied set is never stored; it is computed from the append-only
//! `wal_step` rows every time it is needed — by `reconcile::plan` (what to diff
//! the next scroll against), by recovery, and to rebuild the `AppliedState`
//! cache. Because it is a pure function of the log, a crash mid-attempt cannot
//! leave a stale snapshot: the same rows always fold to the same set.

use chrono::{DateTime, Utc};
use scroll_format::ContentId;

use crate::journal::{
    AttemptPhase, Inverse, Outcome, ReconcileAttempt, Revision, RevisionKind, WalAction, WalStep,
    WalStepState,
};

/// The set of glyphs currently applied to the host, one [`Outcome`] per glyph
/// key. For each key, the latest `Done` step that has not since been `Reversed`
/// wins (so a re-apply's fresh inverse supersedes an older one, and a
/// reverse-then-reapply is applied again). Only `Apply` steps survive the final
/// filter: a `Reverse` step's terminal `Done` records that an undo completed, not
/// that a glyph is present, so it must not appear in the applied set. `Intended`
/// and `Failed` steps are ignored — neither claims a durable host change.
/// Insertion order of first appearance is preserved so the diff sees glyphs in a
/// stable order.
pub fn applied_outcomes(steps: &[WalStep]) -> Vec<Outcome> {
    use std::collections::{BTreeMap, BTreeSet};
    let cancelled = cancelled_dones(steps);
    let mut latest: BTreeMap<String, &WalStep> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    for step in steps {
        if step.state != WalStepState::Done || cancelled.contains(&step.seq) {
            continue;
        }
        if !latest.contains_key(&step.glyph_key) {
            order.push(step.glyph_key.clone());
        }
        latest.insert(step.glyph_key.clone(), step);
    }
    let mut emitted: BTreeSet<String> = BTreeSet::new();
    order
        .into_iter()
        .filter(|key| emitted.insert(key.clone()))
        .filter_map(|key| latest.get(&key))
        .filter(|step| step.action == WalAction::Apply)
        .map(|step| outcome_of(step))
        .collect()
}

/// The `GET /revisions` history, projected from the settled WAL rather than
/// stored (ADR 0020 §6). Revision 1 is always `Init` (the opening, dated from the
/// earliest attempt); then one `Reconcile` per `Committed` attempt in
/// `reconcile_id` order, numbered `2, 3, …`. Each `Reconcile`'s outcomes are
/// [`applied_outcomes`] folded over the steps up to that attempt's last `seq`
/// (`attempt_boundary_seq`), so the revision shows the applied set as it stood
/// once that attempt settled. The latest revision therefore equals the current
/// applied set by construction. A settled attempt yields exactly one revision, so
/// no crash between committing an attempt and appending a separate revision row
/// can lose one — that window is gone because there is no separate row.
pub fn projected_revisions(attempts: &[ReconcileAttempt], steps: &[WalStep]) -> Vec<Revision> {
    let mut revisions = vec![Revision {
        id: 1,
        created_at: opening_time(attempts),
        kind: RevisionKind::Init,
        scroll_content_id: None,
        outcomes: vec![],
    }];
    let committed = attempts.iter().filter(|a| a.phase == AttemptPhase::Committed);
    for (position, attempt) in committed.enumerate() {
        let boundary = attempt_boundary_seq(steps, attempt.reconcile_id);
        let folded = applied_outcomes(&steps_through(steps, boundary));
        revisions.push(Revision {
            id: 2 + position as u64,
            created_at: attempt.settled_at.unwrap_or(attempt.started_at),
            kind: RevisionKind::Reconcile,
            scroll_content_id: attempt.scroll_content_id,
            outcomes: folded,
        });
    }
    revisions
}

/// One projected revision by id, or `None` if no such revision exists. Projects
/// the whole history and picks the match — the caller needs only one, but the
/// fold is cheap and keeps a single source for the projection.
pub fn projected_revision(attempts: &[ReconcileAttempt], steps: &[WalStep], id: u64) -> Option<Revision> {
    projected_revisions(attempts, steps).into_iter().find(|r| r.id == id)
}

/// The id of the newest projected revision: `1` (`Init`) plus the number of
/// `Committed` attempts. Computed from the attempts alone — the steps are not
/// needed to count revisions, only to fold their outcomes — so `settle` can read
/// the latest id back cheaply after committing.
pub fn latest_revision_id(attempts: &[ReconcileAttempt]) -> Option<u64> {
    let committed = attempts.iter().filter(|a| a.phase == AttemptPhase::Committed).count() as u64;
    Some(1 + committed)
}

fn opening_time(attempts: &[ReconcileAttempt]) -> DateTime<Utc> {
    attempts.iter().map(|a| a.started_at).min().unwrap_or_else(Utc::now)
}

fn attempt_boundary_seq(steps: &[WalStep], reconcile_id: u64) -> u64 {
    steps
        .iter()
        .filter(|s| s.reconcile_id == reconcile_id)
        .map(|s| s.seq)
        .max()
        .unwrap_or(0)
}

fn steps_through(steps: &[WalStep], boundary: u64) -> Vec<WalStep> {
    steps.iter().filter(|s| s.seq <= boundary).cloned().collect()
}

/// The `seq`s of `Done` steps that a later `Reversed` marker undid. Each
/// `Reversed` row is paired to the nearest earlier still-unpaired `Done` for the
/// same `(step_ord, action, reconcile_id)` — the step it reverses. Pairing
/// one-to-one (a `Reversed` claims a specific `Done`, and `!cancelled.contains`
/// prevents two markers claiming the same one) keeps an apply→reverse→apply
/// cycle honest: only the first `Done` is cancelled, and the re-applied `Done`
/// stays live.
fn cancelled_dones(steps: &[WalStep]) -> std::collections::BTreeSet<u64> {
    let mut cancelled = std::collections::BTreeSet::new();
    for marker in steps.iter().filter(|s| s.state == WalStepState::Reversed) {
        if let Some(done) = steps
            .iter()
            .rev()
            .find(|s| {
                s.seq < marker.seq
                    && s.state == WalStepState::Done
                    && s.step_ord == marker.step_ord
                    && s.action == marker.action
                    && s.reconcile_id == marker.reconcile_id
                    && !cancelled.contains(&s.seq)
            })
        {
            cancelled.insert(done.seq);
        }
    }
    cancelled
}

fn outcome_of(step: &WalStep) -> Outcome {
    Outcome {
        op: step.op.clone(),
        cid: applied_cid(step),
        inverse: step.inverse.clone().unwrap_or(Inverse::Nothing),
        changed: step.changed.unwrap_or(false),
    }
}

fn applied_cid(step: &WalStep) -> ContentId {
    match &step.op {
        crate::journal::GlyphOp::Install { cid, .. }
        | crate::journal::GlyphOp::Noop { cid, .. }
        | crate::journal::GlyphOp::Remove { cid, .. } => *cid,
        crate::journal::GlyphOp::Replace { new_cid, old_cid, .. } => match step.action {
            WalAction::Apply => *new_cid,
            WalAction::Reverse => *old_cid,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::GlyphOp;
    use scroll_format::Glyph;

    fn apt(name: &str) -> Glyph {
        Glyph::AptPackage { name: name.into() }
    }

    fn cid(glyph: &Glyph) -> ContentId {
        scroll_format::content_id_of_glyph(glyph)
    }

    fn done_apply(seq: u64, ord: u64, glyph: &Glyph) -> WalStep {
        WalStep {
            seq,
            reconcile_id: 1,
            step_ord: ord,
            glyph_key: glyph.key(),
            action: WalAction::Apply,
            state: WalStepState::Done,
            op: GlyphOp::Install { cid: cid(glyph), glyph: glyph.clone() },
            inverse: Some(Inverse::RemoveAptPackage { name: match glyph {
                Glyph::AptPackage { name } => name.clone(),
                _ => unreachable!(),
            } }),
            changed: Some(true),
            unit_path: vec![],
            at: chrono::Utc::now(),
        }
    }

    fn reversed(seq: u64, ord: u64, glyph: &Glyph) -> WalStep {
        WalStep {
            state: WalStepState::Reversed,
            action: WalAction::Apply,
            ..done_apply(seq, ord, glyph)
        }
    }

    #[test]
    fn one_done_apply_is_applied() {
        let steps = vec![done_apply(1, 0, &apt("nginx"))];
        let applied = applied_outcomes(&steps);
        assert_eq!(applied.len(), 1);
        assert_eq!(applied[0].op.key(), "apt:nginx");
    }

    #[test]
    fn a_done_then_reversed_key_is_not_applied() {
        let steps = vec![done_apply(1, 0, &apt("nginx")), reversed(2, 0, &apt("nginx"))];
        assert!(applied_outcomes(&steps).is_empty());
    }

    #[test]
    fn re_applied_after_reverse_is_applied_again() {
        let steps = vec![
            done_apply(1, 0, &apt("nginx")),
            reversed(2, 0, &apt("nginx")),
            done_apply(3, 6, &apt("nginx")),
        ];
        assert_eq!(applied_outcomes(&steps).len(), 1);
    }

    #[test]
    fn intended_and_failed_do_not_count_as_applied() {
        let mut intended = done_apply(1, 0, &apt("nginx"));
        intended.state = WalStepState::Intended;
        let mut failed = done_apply(2, 0, &apt("pg"));
        failed.state = WalStepState::Failed;
        assert!(applied_outcomes(&[intended, failed]).is_empty());
    }

    #[test]
    fn later_done_wins_the_inverse() {
        let mut first = done_apply(1, 0, &apt("nginx"));
        first.inverse = Some(Inverse::Nothing);
        let second = done_apply(2, 0, &apt("nginx"));
        let applied = applied_outcomes(&[first, second]);
        assert_eq!(applied[0].inverse, Inverse::RemoveAptPackage { name: "nginx".into() });
    }
}
