//! The write path: turn a manifest into host changes, durably and reversibly
//! (ADR 0020). The foreman selects this host's scroll, diffs it against the
//! currently-applied set (`wal::applied_outcomes`, folded from the WAL), and
//! enacts the resulting ops through the [`Reconciler`] port — writing a
//! write-ahead log around every side effect.
//!
//! **The bracketing invariant.** Every side effect is framed by two durable
//! writes: an `Intended` [`WalStep`](crate::journal::WalStep) row is committed
//! *before* `Reconciler::apply`/`reverse` is called, and a `Done`/`Failed` row
//! *after* it returns (see [`Foreman::enact_apply`]/[`Foreman::enact_reverse`]).
//! A crash can therefore land the daemon in "an effect was intended but its
//! outcome was never recorded" — but never in "an effect happened and golem has
//! no trace of it." That gap is what recovery closes; it is why the intent row
//! comes first.
//!
//! **Recovery.** On construction, and again under the write lock before each
//! reconcile, [`Foreman::recover`] settles any interrupted attempt before a new
//! manifest is allowed in (ingest is gated on the latest attempt being settled,
//! so a fresh manifest can never clobber an in-progress reversal). Recovery:
//! [`Foreman::redrive_intended`] re-drives every `Intended`-without-terminal step
//! idempotently (reconcilers observe host state first, so re-running converges
//! whether or not the interrupted call took effect), then
//! [`Foreman::rollback_attempt`] reverses the attempt's still-applied steps in
//! reverse order. Both are resumable: a step already `Reversed` is never reversed
//! again, so a rollback continues from wherever the log shows it stopped.
//!
//! Recovery and the reconcile write path share one `write` mutex, so only one
//! attempt is ever in flight and recovery never races an incoming manifest.

use anyhow::{bail, Result};
use scroll_format::{from_bytes, AddressedScroll, ContentId, Entry, Glyph, Scroll};
use std::sync::Mutex;
use std::time::Duration;
use tracing::{debug, info, warn};

use crate::config::RetryConfig;
use crate::journal::{
    AppliedState, AttemptPhase, GlyphOp, Inverse, Outcome, ReconcileAttempt, Revision, WalAction,
    WalStep, WalStepState,
};
use crate::planroom::PlanRoom;
use crate::reconcile::plan;
use crate::reconciler::{EnactError, EnactResult, Reconciler};
use crate::wal::applied_outcomes;

pub struct Foreman {
    host: String,
    planroom: Box<dyn PlanRoom>,
    reconciler: Box<dyn Reconciler>,
    retry: RetryConfig,
    write: Mutex<()>,
}

pub struct SelectedScroll {
    pub content_id: ContentId,
    pub scroll: Scroll,
}

const UNIT_DIRECTORIES: &[&str] = &["/etc/systemd/system", "/etc/containers/systemd"];

impl Foreman {
    pub fn new(host: String, planroom: Box<dyn PlanRoom>, reconciler: Box<dyn Reconciler>) -> Self {
        let foreman = Self {
            host,
            planroom,
            reconciler,
            retry: RetryConfig::default(),
            write: Mutex::new(()),
        };
        if let Err(e) = foreman.recover() {
            warn!(?e, "startup recovery failed");
        }
        foreman
    }

    pub fn with_retry_config(mut self, cfg: RetryConfig) -> Self {
        self.retry = cfg;
        self
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn apply_manifest(&self, bytes: &[u8]) -> Result<Revision> {
        let manifest = from_bytes(bytes).map_err(|e| anyhow::anyhow!("{e}"))?;
        let selected = self.select(&manifest.scrolls);
        // NOTE: one info line per apply for the manifest itself (host, this
        // host's scroll content id in hex, glyph count); the per-op and
        // per-revision lines below keep the whole apply to a handful of lines.
        // Only glyph *keys* are ever logged — never file contents or secrets.
        info!(
            host = %self.host,
            scroll = %selected.content_id,
            glyphs = selected.scroll.all_glyphs().len(),
            "manifest ingested"
        );
        self.reconcile(selected)
    }

    fn select(&self, scrolls: &[AddressedScroll]) -> SelectedScroll {
        match scrolls.iter().find(|a| a.scroll.name == self.host) {
            Some(a) => SelectedScroll { content_id: a.content_id, scroll: a.scroll.clone() },
            None => SelectedScroll {
                content_id: scroll_format::content_id(&empty_scroll(&self.host)),
                scroll: empty_scroll(&self.host),
            },
        }
    }

    /// Plan and enact one manifest under the write lock. First recovers any
    /// interrupted attempt, then refuses to proceed if the latest attempt is
    /// still unsettled — this is the ingest gate that stops a new manifest from
    /// overwriting an in-progress reversal (ADR 0020 §3). Diffs the desired
    /// scroll against the WAL-folded applied set, opens an attempt, and enacts.
    /// On success runs config propagation and settles; on failure rolls back the
    /// steps applied this attempt and returns the error, leaving the node at its
    /// last committed state.
    fn reconcile(&self, desired: SelectedScroll) -> Result<Revision> {
        let _w = self.write.lock().unwrap();
        self.recover_locked()?;
        if let Some(attempt) = self.planroom.latest_attempt()? {
            if !attempt.phase.is_settled() {
                bail!("reconcile {} is unsettled ({:?}); refusing new manifest", attempt.reconcile_id, attempt.phase);
            }
        }
        let prior = applied_outcomes(&self.planroom.wal_steps()?);
        let ops = plan(&prior, &desired.scroll);

        let attempt = self.planroom.open_attempt(Some(desired.content_id))?;
        self.planroom.set_attempt_phase(attempt.reconcile_id, AttemptPhase::Enacting)?;

        match self.enact(attempt.reconcile_id, &ops, &prior) {
            Ok(()) => {
                self.propagate_config(attempt.reconcile_id)?;
                self.settle(attempt.reconcile_id, &desired)
            }
            Err(e) => {
                self.rollback_attempt(attempt.reconcile_id)?;
                self.planroom.set_attempt_phase(attempt.reconcile_id, AttemptPhase::RolledBack)?;
                self.cache_applied_state()?;
                Err(e)
            }
        }
    }

    /// Run each planned op in order, one WAL step per side effect. `Noop` touches
    /// nothing (and writes no step, so the prior `Done` and its inverse stay the
    /// latest for that key — this is why ADR 0020 subsumes
    /// `preserve_prior_inverses`). A `Replace` on a glyph that
    /// [`replaces_in_place`] allows is a single `Apply` of the new version whose
    /// captured inverse restores the old — no window where the resource is
    /// absent; every other `Replace` and every `Remove` reverses the prior
    /// outcome first. The first failing step propagates its error, and the caller
    /// rolls the attempt back.
    fn enact(&self, reconcile_id: u64, ops: &[GlyphOp], prior: &[Outcome]) -> Result<()> {
        // NOTE: Plan 1 placeholder — every step records the host-root path as its
        // `unit_path` because enact still walks one flat op list, not per-leaf
        // units (ADR 0031 §6). The per-unit enact (ADR 0029 revision / Plan 2)
        // replaces this with each op's true leaf name-path. `unit_path` is carried
        // for reporting, not consulted by recovery, so the placeholder is inert.
        let unit_path = [self.host.clone()];
        for (ord, op) in ops.iter().enumerate() {
            let ord = ord as u64;
            match op {
                // NOTE: meaningful ops (install/replace/remove) log at info so an
                // apply's effect is visible in one glance; `Noop` is deliberately
                // debug so an unchanged reconcile stays a couple of lines rather
                // than one per glyph. The value logged is always the glyph key.
                GlyphOp::Noop { .. } => {
                    debug!(key = %op.key(), "noop");
                }
                GlyphOp::Install { cid, glyph } => {
                    info!(key = %op.key(), "install");
                    self.enact_apply(reconcile_id, ord, op, glyph, *cid, None, &unit_path)?;
                }
                GlyphOp::Replace { old_cid, new_cid, glyph } => {
                    info!(key = %op.key(), "replace");
                    if replaces_in_place(glyph) {
                        self.enact_apply(reconcile_id, ord, op, glyph, *new_cid, None, &unit_path)?;
                    } else {
                        let prior_outcome = self.prior_outcome(prior, &op.key(), *old_cid, glyph);
                        self.enact_reverse(reconcile_id, ord, op, &prior_outcome, &unit_path)?;
                        self.enact_apply(reconcile_id, ord, op, glyph, *new_cid, None, &unit_path)?;
                    }
                }
                GlyphOp::Remove { cid, glyph } => {
                    info!(key = %op.key(), "remove");
                    let prior_outcome = self.prior_outcome(prior, &op.key(), *cid, glyph);
                    self.enact_reverse(reconcile_id, ord, op, &prior_outcome, &unit_path)?;
                }
            }
        }
        Ok(())
    }

    /// One bracketed `apply`: append `Intended`, call the reconciler through the
    /// retry spine, then append `Done` with the captured inverse and `changed`,
    /// or `Failed`. The `Intended` write is committed before the reconciler runs,
    /// so a crash across the call leaves a recoverable trace (ADR 0020 §2).
    #[allow(clippy::too_many_arguments)]
    fn enact_apply(
        &self,
        reconcile_id: u64,
        ord: u64,
        op: &GlyphOp,
        glyph: &Glyph,
        cid: ContentId,
        intended_inverse: Option<&Inverse>,
        unit_path: &[String],
    ) -> Result<()> {
        self.planroom.append_wal_step(
            reconcile_id,
            ord,
            &op.key(),
            WalAction::Apply,
            WalStepState::Intended,
            op,
            intended_inverse,
            None,
            unit_path,
        )?;
        match self.attempt(op, || self.reconciler.apply(glyph, cid)) {
            Ok(outcome) => {
                self.planroom.append_wal_step(
                    reconcile_id,
                    ord,
                    &op.key(),
                    WalAction::Apply,
                    WalStepState::Done,
                    op,
                    Some(&outcome.inverse),
                    Some(outcome.changed),
                    unit_path,
                )?;
                Ok(())
            }
            Err(e) => {
                self.planroom.append_wal_step(
                    reconcile_id,
                    ord,
                    &op.key(),
                    WalAction::Apply,
                    WalStepState::Failed,
                    op,
                    None,
                    None,
                    unit_path,
                )?;
                Err(e)
            }
        }
    }

    /// One bracketed `reverse`: append `Intended` carrying the prior outcome's
    /// inverse (the state to restore), reverse through the retry spine, then
    /// append `Done`/`Failed`. Same intent-before ordering as
    /// [`Foreman::enact_apply`].
    fn enact_reverse(
        &self,
        reconcile_id: u64,
        ord: u64,
        op: &GlyphOp,
        prior_outcome: &Outcome,
        unit_path: &[String],
    ) -> Result<()> {
        self.planroom.append_wal_step(
            reconcile_id,
            ord,
            &op.key(),
            WalAction::Reverse,
            WalStepState::Intended,
            op,
            Some(&prior_outcome.inverse),
            None,
            unit_path,
        )?;
        match self.attempt_reverse(op, prior_outcome) {
            Ok(()) => {
                self.planroom.append_wal_step(
                    reconcile_id,
                    ord,
                    &op.key(),
                    WalAction::Reverse,
                    WalStepState::Done,
                    op,
                    Some(&prior_outcome.inverse),
                    Some(true),
                    unit_path,
                )?;
                Ok(())
            }
            Err(e) => {
                self.planroom.append_wal_step(
                    reconcile_id,
                    ord,
                    &op.key(),
                    WalAction::Reverse,
                    WalStepState::Failed,
                    op,
                    None,
                    None,
                    unit_path,
                )?;
                Err(e)
            }
        }
    }

    /// The outcome whose inverse a `Reverse`/`Remove` step will consume. Normally
    /// the recorded applied outcome for this key; if none is found (the applied
    /// set does not know this glyph) it falls back to a synthesized inverse from
    /// the glyph itself, so a reverse always has something to restore.
    fn prior_outcome(&self, prior: &[Outcome], key: &str, cid: ContentId, glyph: &Glyph) -> Outcome {
        prior.iter().find(|o| o.op.key() == key).cloned().unwrap_or(Outcome {
            op: GlyphOp::Install { cid, glyph: glyph.clone() },
            cid,
            inverse: crate::reconciler::inverse_of(glyph),
            changed: true,
        })
    }

    /// After a successful enact, restart units whose backing files this attempt
    /// changed (ADR 0020 §5). A unit-directory `file` that was `Replace`d keeps
    /// the `systemdService` unit a `Noop` — the unit resource did not change, but
    /// its input did — so the running unit would otherwise never pick up the new
    /// config. Collect the distinct units mapped from this attempt's changed
    /// files ([`unit_for_config_file`]) and `restart_unit` each, appended as its
    /// own WAL steps (keyed `restart:<unit>`, inverse `Nothing` — a restart of a
    /// running unit has no separate reversal; the unit's lifecycle stays owned by
    /// its `systemdService` step), so a crash mid-propagation recovers like any
    /// other step. Scoped to files golem itself wrote under unit directories, so
    /// it never restarts units for host-managed config golem did not touch.
    fn propagate_config(&self, reconcile_id: u64) -> Result<()> {
        let steps = self.planroom.wal_steps_for(reconcile_id)?;
        let mut units: Vec<String> = Vec::new();
        for step in &steps {
            if step.state != WalStepState::Done || step.changed != Some(true) {
                continue;
            }
            if let Some(path) = changed_file_path(&step.op) {
                if let Some(unit) = unit_for_config_file(&path) {
                    if !units.contains(&unit) {
                        units.push(unit);
                    }
                }
            }
        }
        if units.is_empty() {
            return Ok(());
        }
        let ord = steps.iter().map(|s| s.step_ord).max().map(|m| m + 1).unwrap_or(0);
        // NOTE: same host-root placeholder as `enact` — see the note there.
        let unit_path = [self.host.clone()];
        for (n, unit) in units.into_iter().enumerate() {
            let glyph = Glyph::SystemdService { unit: unit.clone() };
            let cid = scroll_format::content_id_of_glyph(&glyph);
            let op = GlyphOp::Noop { cid, glyph: glyph.clone() };
            let step_ord = ord + n as u64;
            self.planroom.append_wal_step(
                reconcile_id,
                step_ord,
                &format!("restart:{unit}"),
                WalAction::Apply,
                WalStepState::Intended,
                &op,
                Some(&Inverse::Nothing),
                None,
                &unit_path,
            )?;
            let restarted = self.reconciler.restart_unit(&unit);
            let state = match &restarted {
                Ok(()) => WalStepState::Done,
                Err(_) => WalStepState::Failed,
            };
            self.planroom.append_wal_step(
                reconcile_id,
                step_ord,
                &format!("restart:{unit}"),
                WalAction::Apply,
                state,
                &op,
                Some(&Inverse::Nothing),
                Some(false),
                &unit_path,
            )?;
        }
        Ok(())
    }

    /// Close a successful attempt: mark it `Committed`, refresh the applied-state
    /// cache from the WAL fold, and read back the `Reconcile` revision that
    /// committing this attempt now projects. Nothing is appended — marking the
    /// attempt `Committed` *is* what makes the revision exist
    /// (`wal::projected_revisions`), so its outcomes are the same fold as the
    /// cache. History and the applied set agree because both come from the one log.
    fn settle(&self, reconcile_id: u64, desired: &SelectedScroll) -> Result<Revision> {
        self.planroom.set_attempt_phase(reconcile_id, AttemptPhase::Committed)?;
        let outcomes = applied_outcomes(&self.planroom.wal_steps()?);
        self.planroom.put_applied_state(&AppliedState {
            scroll_content_id: desired.content_id,
            scroll: desired.scroll.clone(),
            outcomes,
        })?;
        let revision = self.planroom
            .revision(self.planroom.latest_revision_id()?.expect("a committed attempt projects a revision"))
            .map(|rev| rev.expect("the latest revision id resolves"))?;
        // NOTE: the closing info line of an apply — the revision id this attempt
        // projected and how many outcomes it holds.
        info!(revision = revision.id, outcomes = revision.outcomes.len(), "revision recorded");
        Ok(revision)
    }

    /// Rewrite the `applied_state` cache row from the current WAL fold, reusing
    /// the last cached scroll for the fields the WAL does not carry. Called after
    /// a rollback and after recovery so the cache tracks the authoritative log;
    /// does nothing when there is neither a prior cache nor any applied outcome.
    fn cache_applied_state(&self) -> Result<()> {
        let outcomes = applied_outcomes(&self.planroom.wal_steps()?);
        let prior = self.planroom.applied_state()?;
        if prior.is_none() && outcomes.is_empty() {
            return Ok(());
        }
        let scroll = prior.map(|a| a.scroll).unwrap_or_else(|| empty_scroll(&self.host));
        self.planroom.put_applied_state(&AppliedState {
            scroll_content_id: scroll_format::content_id(&scroll),
            scroll,
            outcomes,
        })
    }

    /// Undo every still-applied step of an attempt, latest first, marking each
    /// `Reversed`. Resumable and idempotent: it re-reads the WAL each iteration
    /// and [`next_reversible`] only returns a `Done` step not already `Reversed`,
    /// so a rollback interrupted by a crash continues from where the log stopped
    /// and never reverses a step twice (ADR 0020 §3). A failed undo is logged and
    /// the step is still marked `Reversed` — the reconcilers are idempotent, so a
    /// re-drive on the next recovery converges. Reversing an `Apply` runs
    /// `reverse`; reversing a `Reverse` re-`apply`s, restoring the old version.
    fn rollback_attempt(&self, reconcile_id: u64) -> Result<()> {
        self.planroom.set_attempt_phase(reconcile_id, AttemptPhase::RollingBack)?;
        loop {
            let steps = self.planroom.wal_steps_for(reconcile_id)?;
            let Some(target) = next_reversible(&steps) else { break };
            let cid = applied_cid_of(&target.op, target.action);
            let outcome = Outcome {
                op: target.op.clone(),
                cid,
                inverse: target.inverse.clone().unwrap_or(Inverse::Nothing),
                changed: target.changed.unwrap_or(false),
            };
            let undone = match target.action {
                WalAction::Apply => self.reconciler.reverse(&outcome),
                WalAction::Reverse => self.reconciler.apply(target.op.glyph(), cid).map(|_| ()),
            };
            if let Err(e) = undone {
                warn!(?e, "rollback step failed");
            }
            self.planroom.append_wal_step(
                reconcile_id,
                target.step_ord,
                &target.glyph_key,
                target.action,
                WalStepState::Reversed,
                &target.op,
                target.inverse.as_ref(),
                target.changed,
                &target.unit_path,
            )?;
        }
        Ok(())
    }

    fn attempt(&self, op: &GlyphOp, mut run: impl FnMut() -> EnactResult<Outcome>) -> Result<Outcome> {
        for n in 1..=self.retry.max_attempts {
            match run() {
                Ok(outcome) => return Ok(outcome),
                Err(EnactError::Fatal(msg)) => bail!("{op:?}: fatal: {msg}"),
                Err(EnactError::Retryable(msg)) if n == self.retry.max_attempts => {
                    bail!("{op:?}: gave up after {n} attempts: {msg}")
                }
                Err(EnactError::Retryable(msg)) => {
                    warn!(?op, attempt = n, "retryable failure: {msg}");
                    std::thread::sleep(Duration::from_millis(self.retry.base_delay_ms));
                }
            }
        }
        unreachable!("loop returns or bails")
    }

    fn attempt_reverse(&self, op: &GlyphOp, outcome: &Outcome) -> Result<()> {
        for n in 1..=self.retry.max_attempts {
            match self.reconciler.reverse(outcome) {
                Ok(()) => return Ok(()),
                Err(EnactError::Fatal(msg)) => bail!("{op:?}: fatal: {msg}"),
                Err(EnactError::Retryable(msg)) if n == self.retry.max_attempts => {
                    bail!("{op:?}: gave up after {n} attempts: {msg}")
                }
                Err(EnactError::Retryable(msg)) => {
                    warn!(?op, attempt = n, "retryable failure: {msg}");
                    std::thread::sleep(Duration::from_millis(self.retry.base_delay_ms));
                }
            }
        }
        unreachable!("loop returns or bails")
    }

    /// Settle any interrupted attempt, under the write lock. Called once at
    /// startup (from [`Foreman::new`]) and again by [`Foreman::reconcile`] before
    /// each manifest, so recovery is always a precondition of ingest.
    pub fn recover(&self) -> Result<()> {
        let _w = self.write.lock().unwrap();
        self.recover_locked()
    }

    /// The recovery algorithm (ADR 0020 §3), assuming the write lock is held. A
    /// settled latest attempt needs nothing but a cache refresh. An unsettled one
    /// died mid-flight: re-drive its incomplete `Intended` steps
    /// ([`Foreman::redrive_intended`]), roll the attempt back
    /// ([`Foreman::rollback_attempt`]) so the node returns to its last committed
    /// applied set, mark it `RolledBack`, and rebuild the cache. A rollback
    /// already in progress resumes rather than restarts, because
    /// `rollback_attempt` skips already-`Reversed` steps.
    fn recover_locked(&self) -> Result<()> {
        let Some(attempt) = self.planroom.latest_attempt()? else { return Ok(()) };
        if attempt.phase.is_settled() {
            return self.cache_applied_state();
        }
        self.redrive_intended(&attempt)?;
        self.rollback_attempt(attempt.reconcile_id)?;
        self.planroom.set_attempt_phase(attempt.reconcile_id, AttemptPhase::RolledBack)?;
        self.cache_applied_state()
    }

    /// Resolve every `Intended` step of an interrupted attempt that has no
    /// terminal row — the steps golem crashed across, where the side effect may
    /// or may not have happened. Re-run each idempotently (`apply` re-`apply`s and
    /// re-captures the inverse; `reverse` re-reverses) and record the result as
    /// the step's `Done`/`Failed`, so rollback afterward sees a consistent log.
    /// Safe because every reconciler observes host state first, so re-running
    /// converges whether or not the interrupted call took effect (ADR 0020 §3).
    fn redrive_intended(&self, attempt: &ReconcileAttempt) -> Result<()> {
        let steps = self.planroom.wal_steps_for(attempt.reconcile_id)?;
        for step in &steps {
            if step.state != WalStepState::Intended || has_terminal(&steps, step) {
                continue;
            }
            let redriven = match step.action {
                WalAction::Apply => {
                    let cid = applied_cid_of(&step.op, WalAction::Apply);
                    self.reconciler.apply(step.op.glyph(), cid).map(Some)
                }
                WalAction::Reverse => {
                    let outcome = Outcome {
                        op: step.op.clone(),
                        cid: applied_cid_of(&step.op, WalAction::Reverse),
                        inverse: step.inverse.clone().unwrap_or(Inverse::Nothing),
                        changed: step.changed.unwrap_or(true),
                    };
                    self.reconciler.reverse(&outcome).map(|_| None)
                }
            };
            match redriven {
                Ok(outcome) => {
                    let (inverse, changed) = match step.action {
                        WalAction::Apply => {
                            let o = outcome.expect("apply returns an outcome");
                            (o.inverse, o.changed)
                        }
                        WalAction::Reverse => {
                            (step.inverse.clone().unwrap_or(Inverse::Nothing), true)
                        }
                    };
                    self.planroom.append_wal_step(
                        attempt.reconcile_id,
                        step.step_ord,
                        &step.glyph_key,
                        step.action,
                        WalStepState::Done,
                        &step.op,
                        Some(&inverse),
                        Some(changed),
                        &step.unit_path,
                    )?;
                }
                Err(_) => {
                    self.planroom.append_wal_step(
                        attempt.reconcile_id,
                        step.step_ord,
                        &step.glyph_key,
                        step.action,
                        WalStepState::Failed,
                        &step.op,
                        None,
                        None,
                        &step.unit_path,
                    )?;
                }
            }
        }
        Ok(())
    }

    /// The currently-applied state, with `outcomes` always the live WAL fold —
    /// the cache row supplies only the scroll and its content id. So a caller sees
    /// the authoritative applied set even if the cache row lags. `None` only when
    /// nothing has ever been applied and no cache exists.
    pub fn applied_state(&self) -> Result<Option<AppliedState>> {
        let outcomes = applied_outcomes(&self.planroom.wal_steps()?);
        match self.planroom.applied_state()? {
            Some(cached) => Ok(Some(AppliedState { outcomes, ..cached })),
            None if outcomes.is_empty() => Ok(None),
            None => Ok(Some(AppliedState {
                scroll_content_id: scroll_format::content_id(&empty_scroll(&self.host)),
                scroll: empty_scroll(&self.host),
                outcomes,
            })),
        }
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

// A leaf scroll (ADR 0031 §1) holding no glyphs — the desired state that diffs
// every currently-applied glyph to a `Remove`, used when this host has no scroll
// in the manifest.
fn empty_scroll(host: &str) -> Scroll {
    Scroll { name: host.to_string(), policy: None, contents: scroll_format::Contents::Glyphs(vec![]) }
}

/// Whether a `Replace` on this glyph can update in place — one `Apply` whose
/// captured inverse restores the prior version — instead of reverse-then-apply
/// (ADR 0020 §4). Only `Filesystem::File`: an atomic overwrite has no window
/// where the file is absent and stays exactly reversible. `aptPackage` and
/// `systemdService` have distinct reverse/re-apply effects (a package
/// remove/install, a unit stop/start) and must stay reverse-then-apply.
fn replaces_in_place(glyph: &Glyph) -> bool {
    matches!(glyph, Glyph::Filesystem { entry: Entry::File { .. }, .. })
}

fn applied_cid_of(op: &GlyphOp, action: WalAction) -> ContentId {
    match op {
        GlyphOp::Install { cid, .. } | GlyphOp::Noop { cid, .. } | GlyphOp::Remove { cid, .. } => *cid,
        GlyphOp::Replace { new_cid, old_cid, .. } => match action {
            WalAction::Apply => *new_cid,
            WalAction::Reverse => *old_cid,
        },
    }
}

/// Whether an `Intended` step has any later terminal row (`Done`/`Failed`/
/// `Reversed`) for the same step, matched by `step_ord`+`action` since a step's
/// rows share those. An `Intended` with no terminal successor is what
/// [`Foreman::redrive_intended`] must re-drive.
fn has_terminal(steps: &[WalStep], intended: &WalStep) -> bool {
    steps.iter().any(|s| {
        s.seq > intended.seq
            && s.step_ord == intended.step_ord
            && s.action == intended.action
            && matches!(s.state, WalStepState::Done | WalStepState::Failed | WalStepState::Reversed)
    })
}

/// The latest `Done` step of the attempt not yet `Reversed` — the next one a
/// rollback should undo. Scanning newest-first reverses in the opposite order to
/// application (LIFO), and skipping already-`Reversed` steps is what makes
/// [`Foreman::rollback_attempt`] resumable.
fn next_reversible(steps: &[WalStep]) -> Option<&WalStep> {
    steps
        .iter()
        .rev()
        .find(|s| s.state == WalStepState::Done && !reversed_after(steps, s))
}

fn reversed_after(steps: &[WalStep], done: &WalStep) -> bool {
    steps.iter().any(|s| {
        s.seq > done.seq
            && s.step_ord == done.step_ord
            && s.action == done.action
            && s.state == WalStepState::Reversed
    })
}

pub(crate) fn resolve_retry(base: &RetryConfig, policy_chain: &[&scroll_format::Policy]) -> RetryConfig {
    let mut cfg = *base;
    for policy in policy_chain {
        if let Some(v) = policy.base_delay_ms {
            cfg.base_delay_ms = v;
        }
        if let Some(v) = policy.backoff_multiplier {
            cfg.backoff_multiplier = v;
        }
        if let Some(v) = policy.max_delay_ms {
            cfg.max_delay_ms = v;
        }
        if let Some(v) = policy.jitter_fraction {
            cfg.jitter_fraction = v;
        }
        if let Some(v) = policy.max_attempts {
            cfg.max_attempts = v;
        }
        if let Some(v) = policy.max_elapsed_ms {
            cfg.max_elapsed_ms = v;
        }
        if let Some(v) = policy.on_exhaust {
            cfg.on_exhaust = match v {
                scroll_format::OnExhaust::Rollback => crate::config::OnExhaustConfig::Rollback,
                scroll_format::OnExhaust::Keep => crate::config::OnExhaustConfig::Keep,
            };
        }
    }
    cfg
}

fn changed_file_path(op: &GlyphOp) -> Option<String> {
    match op.glyph() {
        Glyph::Filesystem { path, entry: Entry::File { .. } } => Some(path.clone()),
        _ => None,
    }
}

/// The systemd unit a golem-written config file belongs to, or `None` if the
/// path is not under a unit directory (ADR 0020 §5). Location is the whole
/// signal: only files under [`UNIT_DIRECTORIES`] are treated as unit config. A
/// drop-in under `foo.service.d/` maps to `foo.service`; a Podman quadlet
/// `foo.container` maps to its generated `foo.service`; a `foo.service` file maps
/// to itself.
fn unit_for_config_file(path: &str) -> Option<String> {
    let under_unit_dir = UNIT_DIRECTORIES.iter().any(|dir| path.starts_with(dir));
    if !under_unit_dir {
        return None;
    }
    if let Some(component) = path.find(".service.d/") {
        let stem = &path[..component];
        let name = stem.rsplit('/').next()?;
        return Some(format!("{name}.service"));
    }
    let file = path.rsplit('/').next()?;
    if let Some(stem) = file.strip_suffix(".container") {
        return Some(format!("{stem}.service"));
    }
    if file.ends_with(".service") {
        return Some(file.to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::{Inverse, RevisionKind};
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

    fn retry_config(max_attempts: u32) -> RetryConfig {
        RetryConfig { max_attempts, base_delay_ms: 0, ..Default::default() }
    }

    fn foreman(host: &str, reconciler: Box<dyn Reconciler>) -> Foreman {
        Foreman::new(host.into(), Box::new(MemoryPlanRoom::new()), reconciler)
            .with_retry_config(retry_config(3))
    }

    fn apt(name: &str) -> Glyph {
        Glyph::AptPackage { name: name.into() }
    }

    fn manifest(scrolls: Vec<Scroll>) -> Vec<u8> {
        scroll_format::to_bytes(&Manifest::from_scrolls(scrolls, "test"))
    }

    fn scroll(host: &str, glyphs: Vec<Glyph>) -> Scroll {
        Scroll { name: host.into(), policy: None, contents: scroll_format::Contents::Glyphs(glyphs) }
    }

    #[test]
    fn with_retry_config_is_stored() {
        let foreman =
            Foreman::new("h".into(), Box::new(MemoryPlanRoom::new()), Box::new(Recorder::default()))
                .with_retry_config(crate::config::RetryConfig {
                    max_attempts: 9,
                    ..Default::default()
                });
        assert_eq!(foreman.retry.max_attempts, 9);
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
        assert!(rec.calls().is_empty(), "a Noop enacts no side effect");
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
            .with_retry_config(retry_config(1));
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
    fn resolve_retry_uses_config_when_no_policy() {
        let base = crate::config::RetryConfig { max_attempts: 5, ..Default::default() };
        let eff = super::resolve_retry(&base, &[]);
        assert_eq!(eff.max_attempts, 5);
        assert_eq!(eff.on_exhaust, crate::config::OnExhaustConfig::Rollback);
    }

    #[test]
    fn resolve_retry_leaf_overrides_ancestor_overrides_config() {
        use scroll_format::{OnExhaust, Policy};
        let base = crate::config::RetryConfig { max_attempts: 5, ..Default::default() };
        let ancestor = Policy { max_attempts: Some(8), on_exhaust: Some(OnExhaust::Rollback), ..Policy::default() };
        let leaf = Policy { on_exhaust: Some(OnExhaust::Keep), ..Policy::default() };
        let eff = super::resolve_retry(&base, &[&ancestor, &leaf]);
        assert_eq!(eff.max_attempts, 8);
        assert_eq!(eff.on_exhaust, crate::config::OnExhaustConfig::Keep);
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
