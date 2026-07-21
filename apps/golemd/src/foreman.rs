//! The reconcile loop (ADR 0014 §3). One manifest in: select this host's scroll,
//! diff it against the last applied state (`reconcile::plan`), enact each glyph
//! op through the `Reconciler` port with the retry spine, and journal the
//! ordered outcomes. All-or-nothing: if any op fails fatally or exhausts its
//! retries, the ops already applied this reconcile are undone LIFO and nothing
//! is persisted, so the node stays at its last good scroll.

use anyhow::{bail, Result};
use scroll_format::{from_bytes, AddressedScroll, ContentId, Scroll};
use std::sync::Mutex;
use std::time::Duration;
use tracing::warn;

use crate::journal::{AppliedState, GlyphOp, Outcome, Revision, RevisionKind};
use crate::planroom::PlanRoom;
use crate::reconcile::plan;
use crate::reconciler::{EnactError, EnactResult, Reconciler};

pub struct Foreman {
    host: String,
    planroom: Box<dyn PlanRoom>,
    reconciler: Box<dyn Reconciler>,
    max_attempts: u32,
    retry_delay: Duration,
    write: Mutex<()>,
}

/// This host's scroll from the manifest, with its content id. A manifest with
/// no scroll named for this host yields an empty scroll (removing everything).
pub struct SelectedScroll {
    pub content_id: ContentId,
    pub scroll: Scroll,
}

/// One entry on the undo stack built as ops succeed, replayed in reverse on a
/// mid-reconcile failure: `Reverse` undoes a glyph just applied; `Reapply`
/// re-installs the prior glyph that a `Remove`/`Replace` had reversed.
enum UndoStep {
    Reverse(Outcome),
    Reapply(scroll_format::Glyph, ContentId),
}

impl Foreman {
    pub fn new(host: String, planroom: Box<dyn PlanRoom>, reconciler: Box<dyn Reconciler>) -> Self {
        Self {
            host,
            planroom,
            reconciler,
            max_attempts: 5,
            retry_delay: Duration::from_millis(200),
            write: Mutex::new(()),
        }
    }

    pub fn with_retry(mut self, max_attempts: u32, retry_delay: Duration) -> Self {
        self.max_attempts = max_attempts;
        self.retry_delay = retry_delay;
        self
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    /// The ingest entry point: decode the manifest bytes, select this host's
    /// scroll, and reconcile toward it. Returns the journal `Revision` recorded.
    pub fn apply_manifest(&self, bytes: &[u8]) -> Result<Revision> {
        let manifest = from_bytes(bytes).map_err(|e| anyhow::anyhow!("{e}"))?;
        let selected = self.select(&manifest.scrolls);
        self.reconcile(selected)
    }

    /// Pick the scroll named for this host out of the fleet manifest; if the
    /// fleet does not mention this host, an empty scroll (drives full removal).
    fn select(&self, scrolls: &[AddressedScroll]) -> SelectedScroll {
        match scrolls.iter().find(|a| a.scroll.name == self.host) {
            Some(a) => SelectedScroll { content_id: a.content_id, scroll: a.scroll.clone() },
            None => SelectedScroll {
                content_id: scroll_format::content_id(&empty_scroll(&self.host)),
                scroll: empty_scroll(&self.host),
            },
        }
    }

    /// Serialize writers, diff the desired scroll against the prior outcomes,
    /// enact the plan, and — only if the whole plan succeeds — store the new
    /// applied state and append a `Reconcile` revision. A failed `enact` returns
    /// its error with nothing persisted.
    fn reconcile(&self, desired: SelectedScroll) -> Result<Revision> {
        let _w = self.write.lock().unwrap();
        let prior = self.planroom.applied_state()?;
        let prior_outcomes = prior.as_ref().map(|a| a.outcomes.as_slice()).unwrap_or(&[]);
        let ops = plan(prior_outcomes, &desired.scroll);
        // Post-process the enacted outcomes before storing them: a Noop enacted
        // this reconcile carries an empty inverse, so carry the prior real
        // inverse forward for it — otherwise storing it would erase golem's
        // ability to reverse an unchanged glyph. See preserve_prior_inverses.
        let outcomes = preserve_prior_inverses(self.enact(&ops)?, prior_outcomes);
        self.planroom.put_applied_state(&AppliedState {
            scroll_content_id: desired.content_id,
            scroll: desired.scroll,
            outcomes: outcomes.clone(),
        })?;
        self.planroom
            .append_revision(RevisionKind::Reconcile, Some(desired.content_id), &outcomes)
    }

    /// Apply the plan in order, pushing an [`UndoStep`] as each op succeeds and
    /// collecting the [`Outcome`]s to journal. `Replace` is reverse-then-apply so
    /// the host is never left with two versions half-present; `Remove` reverses
    /// the prior outcome and produces no new one. On any failure the undo stack
    /// is replayed LIFO (`rollback`) and the error propagates.
    fn enact(&self, ops: &[GlyphOp]) -> Result<Vec<Outcome>> {
        let mut state: Vec<Outcome> = Vec::new();
        let mut undo: Vec<UndoStep> = Vec::new();
        for op in ops {
            let step = match op {
                GlyphOp::Install { cid, glyph } | GlyphOp::Noop { cid, glyph } => {
                    match self.attempt(op, || self.reconciler.apply(glyph, *cid)) {
                        Ok(mut outcome) => {
                            outcome.op = op.clone();
                            undo.push(UndoStep::Reverse(outcome.clone()));
                            Ok(Some(outcome))
                        }
                        Err(e) => Err(e),
                    }
                }
                GlyphOp::Replace { old_cid, new_cid, glyph } => {
                    let prior = self.prior_outcome(&op.key(), *old_cid, glyph);
                    match self.attempt_reverse(op, &prior) {
                        Ok(()) => {
                            undo.push(UndoStep::Reapply(glyph.clone(), *old_cid));
                            match self.attempt(op, || self.reconciler.apply(glyph, *new_cid)) {
                                Ok(mut outcome) => {
                                    outcome.op = op.clone();
                                    undo.push(UndoStep::Reverse(outcome.clone()));
                                    Ok(Some(outcome))
                                }
                                Err(e) => Err(e),
                            }
                        }
                        Err(e) => Err(e),
                    }
                }
                GlyphOp::Remove { cid, glyph } => {
                    let prior = self.prior_outcome(&op.key(), *cid, glyph);
                    match self.attempt_reverse(op, &prior) {
                        Ok(()) => {
                            undo.push(UndoStep::Reapply(glyph.clone(), *cid));
                            Ok(None)
                        }
                        Err(e) => Err(e),
                    }
                }
            };
            match step {
                Ok(Some(outcome)) => state.push(outcome),
                Ok(None) => {}
                Err(e) => {
                    self.rollback(undo);
                    return Err(e);
                }
            }
        }
        Ok(state)
    }

    /// The recorded outcome for `key` — carrying the real captured inverse — so
    /// a `Remove`/`Replace` reverses exactly what apply did. Falls back to a
    /// synthesized "golem added it" inverse if the journal has no record.
    fn prior_outcome(&self, key: &str, cid: ContentId, glyph: &scroll_format::Glyph) -> Outcome {
        let recorded = self
            .planroom
            .applied_state()
            .ok()
            .flatten()
            .and_then(|a| a.outcomes.into_iter().find(|o| o.op.key() == key));
        recorded.unwrap_or(Outcome {
            op: GlyphOp::Install { cid, glyph: glyph.clone() },
            cid,
            inverse: crate::reconciler::inverse_of(glyph),
            changed: true,
        })
    }

    /// Replay the undo stack in reverse (LIFO) after a mid-reconcile failure,
    /// returning the host to its pre-reconcile state. A failing rollback step is
    /// logged, not propagated — the reconcile already failed.
    fn rollback(&self, undo: Vec<UndoStep>) {
        for step in undo.into_iter().rev() {
            let result = match step {
                UndoStep::Reverse(outcome) => self.reconciler.reverse(&outcome),
                UndoStep::Reapply(glyph, cid) => self.reconciler.apply(&glyph, cid).map(|_| ()),
            };
            if let Err(e) = result {
                warn!(?e, "rollback step failed");
            }
        }
    }

    /// The retry spine around one enact call: retry `Retryable` failures up to
    /// `max_attempts` with `retry_delay` between, give up loudly after the last,
    /// and bail immediately on `Fatal`.
    fn attempt(&self, op: &GlyphOp, mut run: impl FnMut() -> EnactResult<Outcome>) -> Result<Outcome> {
        for n in 1..=self.max_attempts {
            match run() {
                Ok(outcome) => return Ok(outcome),
                Err(EnactError::Fatal(msg)) => bail!("{op:?}: fatal: {msg}"),
                Err(EnactError::Retryable(msg)) if n == self.max_attempts => {
                    bail!("{op:?}: gave up after {n} attempts: {msg}")
                }
                Err(EnactError::Retryable(msg)) => {
                    warn!(?op, attempt = n, "retryable failure: {msg}");
                    std::thread::sleep(self.retry_delay);
                }
            }
        }
        unreachable!("loop returns or bails")
    }

    fn attempt_reverse(&self, op: &GlyphOp, outcome: &Outcome) -> Result<()> {
        for n in 1..=self.max_attempts {
            match self.reconciler.reverse(outcome) {
                Ok(()) => return Ok(()),
                Err(EnactError::Fatal(msg)) => bail!("{op:?}: fatal: {msg}"),
                Err(EnactError::Retryable(msg)) if n == self.max_attempts => {
                    bail!("{op:?}: gave up after {n} attempts: {msg}")
                }
                Err(EnactError::Retryable(msg)) => {
                    warn!(?op, attempt = n, "retryable failure: {msg}");
                    std::thread::sleep(self.retry_delay);
                }
            }
        }
        unreachable!("loop returns or bails")
    }

    pub fn applied_state(&self) -> Result<Option<AppliedState>> {
        self.planroom.applied_state()
    }

    pub fn revisions(&self) -> Result<Vec<Revision>> {
        self.planroom.revisions()
    }

    pub fn revision(&self, id: u64) -> Result<Option<Revision>> {
        self.planroom.revision(id)
    }

    pub fn latest_revision_id(&self) -> Result<Option<u64>> {
        self.planroom.latest_revision_id()
    }
}

fn empty_scroll(host: &str) -> Scroll {
    Scroll { name: host.to_string(), glyphs: vec![] }
}

/// Carry each still-present glyph's real inverse forward across an idempotent
/// re-apply, so the stored applied state keeps — per glyph — the inverse that
/// removes it.
///
/// A `Noop` outcome (the host already matched at the desired CID) carries
/// [`Inverse::Nothing`]: apply changed nothing this reconcile, so it captured
/// nothing to undo. Persisting that empty inverse would clobber the real
/// inverse captured at the glyph's original `Install`, and golem would lose the
/// ability to reverse the glyph forever — recorded state and host diverge
/// permanently. So for a `Noop`, look up the prior recorded outcome by
/// [`Glyph::key`] and keep its inverse.
///
/// `Install` and `Replace` keep their own freshly captured inverse: an install
/// just captured the state that undoes it, and a `Replace`'s inverse is the
/// *new* version's undo (the upgrade's), which must not be overwritten with the
/// prior version's. `Remove` produces no outcome — the glyph is gone.
///
/// Without this, apply → re-apply → apply-empty left a registry container
/// running while golem recorded zero glyphs (reproduced live via the dogfood
/// registry; ADR 0015 addendum).
fn preserve_prior_inverses(outcomes: Vec<Outcome>, prior: &[Outcome]) -> Vec<Outcome> {
    outcomes
        .into_iter()
        .map(|outcome| match outcome.op {
            GlyphOp::Noop { .. } => {
                let key = outcome.op.key();
                match prior.iter().find(|p| p.op.key() == key) {
                    Some(recorded) => Outcome { inverse: recorded.inverse.clone(), ..outcome },
                    None => outcome,
                }
            }
            _ => outcome,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::Inverse;
    use crate::planroom::MemoryPlanRoom;
    use scroll_format::{Glyph, Manifest};
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct Recorder {
        calls: Mutex<Vec<String>>,
        present: Mutex<std::collections::BTreeMap<String, ContentId>>,
    }
    impl Recorder {
        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
    }
    impl Reconciler for Recorder {
        fn apply(&self, glyph: &Glyph, cid: ContentId) -> EnactResult<Outcome> {
            self.calls.lock().unwrap().push(format!("apply {}", glyph.key()));
            self.present.lock().unwrap().insert(glyph.key(), cid);
            Ok(Outcome {
                op: GlyphOp::Install { cid, glyph: glyph.clone() },
                cid,
                inverse: crate::reconciler::inverse_of(glyph),
                changed: true,
            })
        }
        fn reverse(&self, outcome: &Outcome) -> EnactResult<()> {
            self.calls.lock().unwrap().push(format!("reverse {}", outcome.op.key()));
            self.present.lock().unwrap().remove(&outcome.op.key());
            Ok(())
        }
    }

    struct FlakyThenOk {
        fails_left: Mutex<u32>,
        calls: Mutex<u32>,
    }
    impl FlakyThenOk {
        fn new(fails: u32) -> Self {
            Self { fails_left: Mutex::new(fails), calls: Mutex::new(0) }
        }
    }
    impl Reconciler for FlakyThenOk {
        fn apply(&self, glyph: &Glyph, cid: ContentId) -> EnactResult<Outcome> {
            *self.calls.lock().unwrap() += 1;
            let mut left = self.fails_left.lock().unwrap();
            if *left > 0 {
                *left -= 1;
                Err(EnactError::Retryable("flaky".into()))
            } else {
                Ok(Outcome {
                    op: GlyphOp::Install { cid, glyph: glyph.clone() },
                    cid,
                    inverse: crate::reconciler::inverse_of(glyph),
                    changed: true,
                })
            }
        }
        fn reverse(&self, _outcome: &Outcome) -> EnactResult<()> {
            Ok(())
        }
    }

    struct Failing {
        make: fn(String) -> EnactError,
        calls: Mutex<u32>,
    }
    impl Failing {
        fn new(make: fn(String) -> EnactError) -> Self {
            Self { make, calls: Mutex::new(0) }
        }
        fn calls(&self) -> u32 {
            *self.calls.lock().unwrap()
        }
    }
    impl Reconciler for Failing {
        fn apply(&self, _glyph: &Glyph, _cid: ContentId) -> EnactResult<Outcome> {
            *self.calls.lock().unwrap() += 1;
            Err((self.make)("nope".into()))
        }
        fn reverse(&self, _outcome: &Outcome) -> EnactResult<()> {
            Ok(())
        }
    }

    fn foreman(host: &str, reconciler: Box<dyn Reconciler>) -> Foreman {
        Foreman::new(host.into(), Box::new(MemoryPlanRoom::new()), reconciler)
            .with_retry(3, Duration::ZERO)
    }

    fn apt(name: &str) -> Glyph {
        Glyph::AptPackage { name: name.into() }
    }

    fn manifest(scrolls: Vec<Scroll>) -> Vec<u8> {
        scroll_format::to_bytes(&Manifest::from_scrolls(scrolls, "test"))
    }

    fn scroll(host: &str, glyphs: Vec<Glyph>) -> Scroll {
        Scroll { name: host.into(), glyphs }
    }

    #[test]
    fn applies_only_this_hosts_scroll() {
        let rec = Arc::new(Recorder::default());
        let f = foreman("h1", Box::new(rec.clone()));
        let bytes = manifest(vec![
            scroll("h1", vec![apt("nginx")]),
            scroll("h2", vec![apt("other")]),
        ]);
        let rev = f.apply_manifest(&bytes).unwrap();

        assert_eq!(rev.kind, RevisionKind::Reconcile);
        assert_eq!(rec.calls(), vec!["apply apt:nginx"]);
        assert_eq!(f.revisions().unwrap().len(), 2);
    }

    #[test]
    fn missing_host_scroll_is_empty_reconcile() {
        let rec = Arc::new(Recorder::default());
        let f = foreman("h1", Box::new(rec.clone()));
        let bytes = manifest(vec![scroll("h2", vec![apt("other")])]);
        f.apply_manifest(&bytes).unwrap();
        assert!(rec.calls().is_empty());
    }

    #[test]
    fn reapplying_same_scroll_is_noop_but_still_journals() {
        let rec = Arc::new(Recorder::default());
        let f = foreman("h1", Box::new(rec.clone()));
        let bytes = manifest(vec![scroll("h1", vec![apt("nginx")])]);
        f.apply_manifest(&bytes).unwrap();
        rec.calls.lock().unwrap().clear();
        f.apply_manifest(&bytes).unwrap();
        assert_eq!(rec.calls(), vec!["apply apt:nginx"]);
        assert_eq!(f.revisions().unwrap().len(), 3);
    }

    #[test]
    fn removed_glyph_is_reversed() {
        let rec = Arc::new(Recorder::default());
        let f = foreman("h1", Box::new(rec.clone()));
        f.apply_manifest(&manifest(vec![scroll("h1", vec![apt("nginx"), apt("pg")])])).unwrap();
        rec.calls.lock().unwrap().clear();
        f.apply_manifest(&manifest(vec![scroll("h1", vec![apt("nginx")])])).unwrap();
        assert!(rec.calls().contains(&"reverse apt:pg".to_string()));
    }

    #[test]
    fn empty_scroll_removes_everything() {
        let rec = Arc::new(Recorder::default());
        let f = foreman("h1", Box::new(rec.clone()));
        f.apply_manifest(&manifest(vec![scroll("h1", vec![apt("nginx")])])).unwrap();
        rec.calls.lock().unwrap().clear();
        f.apply_manifest(&manifest(vec![scroll("h1", vec![])])).unwrap();
        assert_eq!(rec.calls(), vec!["reverse apt:nginx"]);
        assert!(f.applied_state().unwrap().unwrap().outcomes.is_empty());
    }

    #[test]
    fn retryable_failures_are_retried_until_success() {
        let flaky = Arc::new(FlakyThenOk::new(2));
        let f = foreman("h1", Box::new(flaky.clone()));
        f.apply_manifest(&manifest(vec![scroll("h1", vec![apt("app")])])).unwrap();
        assert_eq!(*flaky.calls.lock().unwrap(), 3);
    }

    #[test]
    fn no_retry_config_attempts_once() {
        let failing = Arc::new(Failing::new(EnactError::Retryable));
        let f = Foreman::new("h1".into(), Box::new(MemoryPlanRoom::new()), Box::new(failing.clone()))
            .with_retry(1, Duration::ZERO);
        assert!(f.apply_manifest(&manifest(vec![scroll("h1", vec![apt("app")])])).is_err());
        assert_eq!(failing.calls(), 1);
    }

    #[test]
    fn exhausted_retries_fail_loudly_and_persist_nothing() {
        let failing = Arc::new(Failing::new(EnactError::Retryable));
        let f = foreman("h1", Box::new(failing.clone()));
        let err = f
            .apply_manifest(&manifest(vec![scroll("h1", vec![apt("app")])]))
            .unwrap_err();
        assert!(err.to_string().contains("gave up"));
        assert_eq!(failing.calls(), 3);
        assert!(f.applied_state().unwrap().is_none());
        assert_eq!(f.revisions().unwrap().len(), 1);
    }

    #[test]
    fn fatal_failure_is_not_retried_and_persists_nothing() {
        let failing = Arc::new(Failing::new(EnactError::Fatal));
        let f = foreman("h1", Box::new(failing.clone()));
        let err = f
            .apply_manifest(&manifest(vec![scroll("h1", vec![apt("app")])]))
            .unwrap_err();
        assert!(err.to_string().contains("fatal"));
        assert_eq!(failing.calls(), 1);
        assert!(f.applied_state().unwrap().is_none());
        assert_eq!(f.revisions().unwrap().len(), 1);
    }

    struct HostModel {
        present: Mutex<std::collections::BTreeMap<String, ContentId>>,
        calls: Mutex<Vec<String>>,
    }
    impl HostModel {
        fn new() -> Self {
            Self { present: Mutex::new(std::collections::BTreeMap::new()), calls: Mutex::new(vec![]) }
        }
        fn present_keys(&self) -> Vec<String> {
            self.present.lock().unwrap().keys().cloned().collect()
        }
    }
    impl Reconciler for HostModel {
        fn apply(&self, glyph: &Glyph, cid: ContentId) -> EnactResult<Outcome> {
            let key = glyph.key();
            let mut present = self.present.lock().unwrap();
            let already = present.get(&key) == Some(&cid);
            present.insert(key.clone(), cid);
            self.calls.lock().unwrap().push(format!("apply {key}"));
            Ok(Outcome {
                op: GlyphOp::Install { cid, glyph: glyph.clone() },
                cid,
                inverse: if already { Inverse::Nothing } else { crate::reconciler::inverse_of(glyph) },
                changed: !already,
            })
        }
        fn reverse(&self, outcome: &Outcome) -> EnactResult<()> {
            match &outcome.inverse {
                Inverse::Nothing => {}
                _ => {
                    self.present.lock().unwrap().remove(&outcome.op.key());
                }
            }
            self.calls.lock().unwrap().push(format!("reverse {}", outcome.op.key()));
            Ok(())
        }
    }

    #[test]
    fn reapply_preserves_real_inverses_so_later_removal_reverts_host() {
        let host = Arc::new(HostModel::new());
        let f = foreman("h1", Box::new(host.clone()));

        let s = manifest(vec![scroll("h1", vec![apt("nginx")])]);
        f.apply_manifest(&s).unwrap();
        f.apply_manifest(&s).unwrap();

        let stored = f.applied_state().unwrap().unwrap();
        let nginx = stored.outcomes.iter().find(|o| o.op.key() == "apt:nginx").unwrap();
        assert_eq!(
            nginx.inverse,
            Inverse::RemoveAptPackage { name: "nginx".into() },
            "re-apply must not overwrite the real inverse with Nothing"
        );

        f.apply_manifest(&manifest(vec![scroll("h1", vec![])])).unwrap();

        assert!(host.present_keys().is_empty(), "removal must revert the host");
        assert!(host.calls.lock().unwrap().contains(&"reverse apt:nginx".to_string()));
        assert!(f.applied_state().unwrap().unwrap().outcomes.is_empty());
    }

    #[test]
    fn partial_failure_rolls_back_applied_outcomes() {
        struct FailSecond {
            calls: Mutex<u32>,
            reversed: Mutex<Vec<String>>,
        }
        impl Reconciler for FailSecond {
            fn apply(&self, glyph: &Glyph, cid: ContentId) -> EnactResult<Outcome> {
                let mut c = self.calls.lock().unwrap();
                *c += 1;
                if *c == 2 {
                    return Err(EnactError::Fatal("boom".into()));
                }
                Ok(Outcome {
                    op: GlyphOp::Install { cid, glyph: glyph.clone() },
                    cid,
                    inverse: crate::reconciler::inverse_of(glyph),
                    changed: true,
                })
            }
            fn reverse(&self, outcome: &Outcome) -> EnactResult<()> {
                self.reversed.lock().unwrap().push(outcome.op.key());
                Ok(())
            }
        }
        let rec = Arc::new(FailSecond { calls: Mutex::new(0), reversed: Mutex::new(vec![]) });
        let f = foreman("h1", Box::new(rec.clone()));
        let err = f
            .apply_manifest(&manifest(vec![scroll("h1", vec![apt("a"), apt("b")])]))
            .unwrap_err();
        assert!(err.to_string().contains("fatal"));
        assert_eq!(*rec.reversed.lock().unwrap(), vec!["apt:a".to_string()]);
        assert!(f.applied_state().unwrap().is_none());
    }
}
