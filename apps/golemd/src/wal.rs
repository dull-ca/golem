//! The fold from the write-ahead log to the currently-applied set (ADR 0020 §1).
//! The applied set is never stored; it is computed from the append-only
//! `wal_step` rows every time it is needed — by `reconcile::plan` (what to diff
//! the next scroll against), by recovery, and to rebuild the `AppliedState`
//! cache. Because it is a pure function of the log, a crash mid-attempt cannot
//! leave a stale snapshot: the same rows always fold to the same set.

use scroll_format::ContentId;

use crate::journal::{Inverse, Outcome, WalAction, WalStep, WalStepState};

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
