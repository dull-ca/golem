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

use anyhow::Result;
use scroll_format::{
    from_bytes, AddressedScroll, ContentId, Contents, Entry, Glyph, LeafUnit, Policy, Scroll,
};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

use crate::config::{OnExhaustConfig, RetryConfig};
use crate::journal::{
    AppliedState, AttemptPhase, GlyphOp, Inverse, Outcome, ReconcileAttempt, Revision, WalAction,
    WalStep, WalStepState,
};
use crate::planroom::PlanRoom;
use crate::reconcile::plan;
use crate::reconciler::{EnactError, Reconciler};
use crate::report::{
    FailClassReport, FailPhase, GlyphAction, GlyphFailure, GlyphLine, GlyphOutcome,
    ReconcileReport, UnitOutcome, UnitReport,
};
use crate::wal::applied_outcomes;

/// The retryability of one failed enact call, tracked in memory across a unit's
/// retry rounds. The WAL records the `Failed` bracket (crash-recovery truth) but
/// not the class — the class is a live-loop concern, so the round loop keeps it
/// alongside each op's ordinal (ADR 0029 §1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum RetryClass {
    Fatal,
    Retryable,
}

/// The classified result of one bracketed reconciler call: `Ok` on success,
/// `Failed` carrying why it failed and its retryability. A reconciler failure is
/// never a `Result::Err` — it lives here and in the WAL `Failed` row — so a
/// failing op no longer aborts the loop (only genuine planroom I/O bails).
#[derive(Debug, Clone)]
pub(crate) enum StepClass {
    Ok,
    Failed(RetryClass, String),
}

/// The terminal classification of a still-failing op after the round loop ends:
/// `Fatal` never retried, `RetriesExhausted` a retryable that hit a limit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum FailClass {
    Fatal,
    RetriesExhausted,
}

/// Which side of the bracket failed — an `apply` or a `reverse`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Phase {
    Enact,
    Reverse,
}

/// One glyph that a leaf unit could not settle, collected after best-effort and
/// retries drained. Mapped into a wire [`GlyphFailure`] by `unit_report_from`.
#[derive(Debug, Clone)]
pub(crate) struct UnitFailure {
    pub glyph_key: String,
    pub unit_path: Vec<String>,
    pub phase: Phase,
    pub class: FailClass,
    pub attempts: u32,
    pub message: String,
    pub rolled_back: bool,
}

/// A leaf unit's fate: the failures it could not settle, whether its
/// `on_exhaust = rollback` undid this attempt's applied glyphs, and the rolled-up
/// unit outcome.
pub(crate) struct UnitResult {
    pub unit_path: Vec<String>,
    pub glyphs: Vec<GlyphLine>,
    pub failures: Vec<UnitFailure>,
    pub outcome: UnitOutcome,
}

fn classify(e: EnactError) -> StepClass {
    match e {
        EnactError::Fatal(m) => StepClass::Failed(RetryClass::Fatal, m),
        EnactError::Retryable(m) => StepClass::Failed(RetryClass::Retryable, m),
    }
}

/// The per-round delay before re-driving a unit's still-failing ops:
/// `min(max_delay_ms, base_delay_ms × backoff_multiplier^(round-1))`, then
/// perturbed by ± `jitter_fraction` (uniform, via `fastrand`) to de-synchronize
/// retries across a fleet (ADR 0029 §3).
pub(crate) fn round_delay(retry: &RetryConfig, round: u32) -> Duration {
    let exp = retry.backoff_multiplier.powi((round - 1) as i32);
    let raw = (retry.base_delay_ms as f64 * exp).min(retry.max_delay_ms as f64);
    let jitter = if retry.jitter_fraction > 0.0 {
        let span = raw * retry.jitter_fraction;
        raw + (fastrand::f64() * 2.0 - 1.0) * span
    } else {
        raw
    };
    Duration::from_millis(jitter.max(0.0) as u64)
}

pub struct Foreman {
    host: String,
    planroom: Box<dyn PlanRoom>,
    reconciler: Box<dyn Reconciler>,
    /// Fleet default (from `golemd.toml`); the per-scroll `policy` cascade
    /// overrides it per unit via `resolve_retry`.
    retry: RetryConfig,
    write: Mutex<()>,
}

pub struct SelectedScroll {
    pub content_id: ContentId,
    pub scroll: Scroll,
}

const UNIT_DIRECTORIES: &[&str] = &["/etc/systemd/system", "/etc/containers/systemd"];

/// The synthetic terminal segment every vanished-removes group appends to its
/// resolved unit path, so the group's `unit_path` is one segment longer than the
/// surviving ancestor it resolved to and therefore disjoint from that ancestor's
/// own path. Without it a group resolving to a present unit's path — a flat host
/// (`[host]`) or a glyph dropped from a still-present unit B (`[host, b]`) — would
/// share that unit's `unit_path`, and a removes-group rollback would reverse the
/// present unit's applied steps. Both those present units are leaves whose path
/// ends at the resolved node, so appending any segment makes the group disjoint.
/// The marker is chosen so a group reports legibly as `… / b / <removes>`; angle
/// brackets are unconventional in an authored scroll name, and Emet does not
/// reserve names, so this is a naming convention rather than an enforced literal.
const REMOVES_SEGMENT: &str = "<removes>";

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

    pub fn apply_manifest(&self, bytes: &[u8]) -> Result<ReconcileReport, ForemanError> {
        let manifest = from_bytes(bytes).map_err(|e| ForemanError::ManifestUndecodable {
            detail: e.to_string(),
        })?;
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
            Some(a) => SelectedScroll {
                content_id: a.content_id,
                scroll: a.scroll.clone(),
            },
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
    fn reconcile(&self, desired: SelectedScroll) -> Result<ReconcileReport, ForemanError> {
        let _w = self.write.lock().unwrap();
        self.recover_locked()
            .map_err(|e| ForemanError::WalUnreadable {
                detail: e.to_string(),
            })?;
        let steps = self
            .planroom
            .wal_steps()
            .map_err(|e| ForemanError::WalUnreadable {
                detail: e.to_string(),
            })?;
        if let Some(attempt) =
            self.planroom
                .latest_attempt()
                .map_err(|e| ForemanError::WalUnreadable {
                    detail: e.to_string(),
                })?
        {
            if !attempt.phase.is_settled() {
                return Err(ForemanError::Internal(anyhow::anyhow!(
                    "reconcile {} is unsettled ({:?}); refusing new manifest",
                    attempt.reconcile_id,
                    attempt.phase
                )));
            }
        }
        let prior = applied_outcomes(&steps);
        let attempt = self
            .planroom
            .open_attempt(Some(desired.content_id))
            .map_err(|e| ForemanError::Internal(e.into()))?;
        self.planroom
            .set_attempt_phase(attempt.reconcile_id, AttemptPhase::Enacting)
            .map_err(|e| ForemanError::Internal(e.into()))?;

        let started = Instant::now();
        let units = desired.scroll.leaf_units();
        let mut unit_reports = Vec::new();
        let mut next_ord: u64 = 0;
        for unit in &units {
            let effective = resolve_retry(&self.retry, &unit.policy_chain);
            let ops: Vec<GlyphOp> = plan(&prior, &leaf_as_scroll(unit))
                .into_iter()
                .filter(|o| !matches!(o, GlyphOp::Remove { .. }))
                .collect();
            let result = self
                .enact_unit(
                    attempt.reconcile_id,
                    &mut next_ord,
                    &ops,
                    &prior,
                    &unit.path,
                    &effective,
                    started,
                )
                .map_err(ForemanError::Internal)?;
            unit_reports.push(unit_report_from(result));
        }

        for group in self
            .plan_vanished_removes(&prior, &desired.scroll, &units)
            .map_err(ForemanError::Internal)?
        {
            let effective = resolve_retry(&self.retry, &group.policy_chain);
            let result = self
                .enact_unit(
                    attempt.reconcile_id,
                    &mut next_ord,
                    &group.ops,
                    &prior,
                    &group.unit_path,
                    &effective,
                    started,
                )
                .map_err(ForemanError::Internal)?;
            unit_reports.push(unit_report_from(result));
        }

        self.propagate_config(attempt.reconcile_id)
            .map_err(|e| ForemanError::Internal(e.into()))?;
        let revision = self
            .settle(attempt.reconcile_id, &desired)
            .map_err(|e| ForemanError::Internal(e.into()))?;
        let report = ReconcileReport::roll_up(revision, unit_reports);
        log_settled(&report);
        Ok(report)
    }

    /// The `Remove` ops for glyphs that belong to no present leaf unit — a unit
    /// that vanished between manifests (ADR 0031 §4). The whole-scroll diff over
    /// `all_glyphs` yields these removes; each is grouped under the longest prefix
    /// of its recorded `unit_path` that still exists as a node in the new scroll
    /// tree — the nearest surviving ancestor — whose policy chain the group
    /// inherits. An empty recorded path (an old host-root placeholder row) falls
    /// back to the host root. The group's own `unit_path` is that resolved path
    /// plus a [`REMOVES_SEGMENT`] terminal, keeping it disjoint from every present
    /// unit's path so a removes-group rollback never reverses a present unit's
    /// steps; policy resolution still runs on the un-suffixed resolved path.
    fn plan_vanished_removes<'a>(
        &self,
        prior: &[Outcome],
        desired: &'a Scroll,
        units: &[LeafUnit<'_>],
    ) -> Result<Vec<RemoveGroup<'a>>> {
        let removes: Vec<GlyphOp> = plan(prior, desired)
            .into_iter()
            .filter(|o| matches!(o, GlyphOp::Remove { .. }))
            .collect();
        if removes.is_empty() {
            return Ok(Vec::new());
        }
        let recorded = self.recorded_unit_paths()?;
        let nodes = node_paths(desired, units);
        let mut groups: Vec<RemoveGroup> = Vec::new();
        for op in removes {
            let recorded_path = recorded.get(&op.key()).cloned().unwrap_or_default();
            let resolved =
                surviving_prefix(&recorded_path, &nodes).unwrap_or_else(|| vec![self.host.clone()]);
            let mut unit_path = resolved.clone();
            unit_path.push(REMOVES_SEGMENT.to_string());
            match groups.iter_mut().find(|g| g.unit_path == unit_path) {
                Some(g) => g.ops.push(op),
                None => {
                    let policy_chain = policy_chain_for_path(desired, &resolved);
                    groups.push(RemoveGroup {
                        unit_path,
                        ops: vec![op],
                        policy_chain,
                    });
                }
            }
        }
        Ok(groups)
    }

    /// Each applied glyph key mapped to the `unit_path` its most recent WAL step
    /// recorded (ADR 0031 §4) — the source the vanished-removes pass groups by.
    fn recorded_unit_paths(&self) -> Result<std::collections::BTreeMap<String, Vec<String>>> {
        let steps = self.planroom.wal_steps()?;
        let mut latest: std::collections::BTreeMap<String, Vec<String>> =
            std::collections::BTreeMap::new();
        for step in &steps {
            latest.insert(step.glyph_key.clone(), step.unit_path.clone());
        }
        Ok(latest)
    }

    /// Enact one leaf unit best-effort (ADR 0029 §1), returning its
    /// [`UnitResult`]. Round 1 runs every op once; between rounds the still-failing
    /// retryable ops are re-driven after [`round_delay`], stopping when nothing
    /// remains, `max_attempts` rounds are reached, or the `max_elapsed_ms`
    /// wall-time budget is spent — whichever trips first. The budget is measured
    /// against `started`, captured once when the attempt opened and shared across
    /// every unit, so `max_elapsed_ms` bounds the whole reconcile's retrying rather
    /// than resetting per unit — a unit reached after the budget is already spent
    /// gets only its opening round (ADR 0029 §3). A `Failed` op never aborts the
    /// loop; its class is tracked in memory (the WAL records the bracket for crash
    /// recovery, not the class). After the loop the unit's `on_exhaust` (`rollback`
    /// | `keep`) settles its fate, scoped to this unit's `unit_path` so a sibling
    /// unit is never touched (ADR 0029 §4, §2).
    fn enact_unit(
        &self,
        reconcile_id: u64,
        next_ord: &mut u64,
        ops: &[GlyphOp],
        prior: &[Outcome],
        unit_path: &[String],
        retry: &RetryConfig,
        started: Instant,
    ) -> Result<UnitResult> {
        // NOTE: `step_ord` is unique across ALL units of an attempt — the shared
        // `next_ord` counter advances by `ops.len()` per unit, never resetting. The
        // WAL grouping predicates (`has_terminal`/`next_reversible`/`reversed_after`,
        // `wal::cancelled_dones`) key on `(step_ord, action)` and depend on it.
        let base_ord = *next_ord;
        *next_ord += ops.len() as u64;
        let mut classes: Vec<StepClass> = Vec::with_capacity(ops.len());
        for (offset, op) in ops.iter().enumerate() {
            classes.push(self.enact_one(
                reconcile_id,
                base_ord + offset as u64,
                op,
                prior,
                unit_path,
                1,
            )?);
        }
        let mut round = 1u32;
        loop {
            let remaining = remaining_ops(ops, &classes);
            if remaining.is_empty() {
                break;
            }
            if round + 1 > retry.max_attempts {
                break;
            }
            if started.elapsed().as_millis() as u64 >= retry.max_elapsed_ms {
                break;
            }
            std::thread::sleep(round_delay(retry, round));
            round += 1;
            for offset in remaining {
                let op = &ops[offset as usize];
                classes[offset as usize] =
                    self.enact_one(reconcile_id, base_ord + offset, op, prior, unit_path, round)?;
            }
        }
        let failures = unit_failures(ops, &classes, unit_path, round);
        for f in &failures {
            error!(
                glyph_key = %f.glyph_key,
                round,
                class = failclass_tag(f.class),
                reason = %f.message,
                "enact failed; giving up"
            );
        }
        let has_failures = !failures.is_empty();
        let rolled_back = has_failures && retry.on_exhaust == OnExhaustConfig::Rollback;
        if rolled_back {
            self.rollback_unit(reconcile_id, unit_path)?;
        }
        let outcome = if !has_failures {
            UnitOutcome::Settled
        } else if rolled_back {
            UnitOutcome::RolledBack
        } else {
            UnitOutcome::Partial
        };
        let glyphs = glyph_lines(ops, &classes, rolled_back, round);
        Ok(UnitResult {
            unit_path: unit_path.to_vec(),
            glyphs,
            failures: failures
                .into_iter()
                .map(|mut f| {
                    f.rolled_back = rolled_back;
                    f
                })
                .collect(),
            outcome,
        })
    }

    /// Enact one op once and return its classified [`StepClass`]. `Noop` touches
    /// nothing and writes no step (so the prior `Done` and inverse stay the latest
    /// for that key — ADR 0020 subsumes `preserve_prior_inverses`). A `Replace`
    /// that [`replaces_in_place`] is a single `Apply` whose captured inverse
    /// restores the old — no window where the resource is absent; every other
    /// `Replace` and every `Remove` reverses the prior outcome first. A reconciler
    /// failure is returned as a `Failed` class (never `Err`); only planroom I/O
    /// bails with `?`.
    fn enact_one(
        &self,
        reconcile_id: u64,
        ord: u64,
        op: &GlyphOp,
        prior: &[Outcome],
        unit_path: &[String],
        round: u32,
    ) -> Result<StepClass> {
        match op {
            GlyphOp::Noop { .. } => {
                debug!(key = %op.key(), "noop");
                Ok(StepClass::Ok)
            }
            GlyphOp::Install { cid, glyph } => {
                info!(key = %op.key(), "install");
                self.enact_apply(reconcile_id, ord, op, glyph, *cid, None, unit_path, round)
            }
            GlyphOp::Replace {
                old_cid,
                new_cid,
                glyph,
            } => {
                info!(key = %op.key(), "replace");
                if replaces_in_place(glyph) {
                    self.enact_apply(
                        reconcile_id,
                        ord,
                        op,
                        glyph,
                        *new_cid,
                        None,
                        unit_path,
                        round,
                    )
                } else {
                    let prior_outcome = self.prior_outcome(prior, &op.key(), *old_cid, glyph);
                    let reversed = self.enact_reverse(
                        reconcile_id,
                        ord,
                        op,
                        &prior_outcome,
                        unit_path,
                        round,
                    )?;
                    if let StepClass::Failed(..) = reversed {
                        return Ok(reversed);
                    }
                    self.enact_apply(
                        reconcile_id,
                        ord,
                        op,
                        glyph,
                        *new_cid,
                        None,
                        unit_path,
                        round,
                    )
                }
            }
            GlyphOp::Remove { cid, glyph } => {
                info!(key = %op.key(), "remove");
                let prior_outcome = self.prior_outcome(prior, &op.key(), *cid, glyph);
                self.enact_reverse(reconcile_id, ord, op, &prior_outcome, unit_path, round)
            }
        }
    }

    /// One bracketed `apply`: append `Intended`, call the reconciler **once**,
    /// then append `Done` with the captured inverse and `changed`, or `Failed`.
    /// The `Intended` write is committed before the reconciler runs, so a crash
    /// across the call leaves a recoverable trace (ADR 0020 §2). A reconciler
    /// failure returns its [`StepClass`] and logs immediately at the `Failed` arm
    /// (glyph key + reason only, never contents/secrets — ADR 0029 §2); it never
    /// aborts the loop. Only a planroom write bails with `?`.
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
        round: u32,
    ) -> Result<StepClass> {
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
        match self.reconciler.apply(glyph, cid) {
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
                Ok(StepClass::Ok)
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
                let class = classify(e);
                log_step_failure(&op.key(), round, &class);
                Ok(class)
            }
        }
    }

    /// One bracketed `reverse`: append `Intended` carrying the prior outcome's
    /// inverse (the state to restore), reverse **once**, then append
    /// `Done`/`Failed`. Same intent-before ordering, single call, and immediate
    /// Failed-arm logging as [`Foreman::enact_apply`].
    #[allow(clippy::too_many_arguments)]
    fn enact_reverse(
        &self,
        reconcile_id: u64,
        ord: u64,
        op: &GlyphOp,
        prior_outcome: &Outcome,
        unit_path: &[String],
        round: u32,
    ) -> Result<StepClass> {
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
        match self.reconciler.reverse(prior_outcome) {
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
                Ok(StepClass::Ok)
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
                let class = classify(e);
                log_step_failure(&op.key(), round, &class);
                Ok(class)
            }
        }
    }

    /// The outcome whose inverse a `Reverse`/`Remove` step will consume. Normally
    /// the recorded applied outcome for this key; if none is found (the applied
    /// set does not know this glyph) it falls back to a synthesized inverse from
    /// the glyph itself, so a reverse always has something to restore.
    fn prior_outcome(
        &self,
        prior: &[Outcome],
        key: &str,
        cid: ContentId,
        glyph: &Glyph,
    ) -> Outcome {
        prior
            .iter()
            .find(|o| o.op.key() == key)
            .cloned()
            .unwrap_or(Outcome {
                op: GlyphOp::Install {
                    cid,
                    glyph: glyph.clone(),
                },
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
    /// it never restarts units for host-managed config golem did not touch. A
    /// changed file whose `Done` a later `Reversed` cancelled — a unit that applied
    /// its config then rolled back under `on_exhaust = rollback` — is excluded via
    /// [`crate::wal::cancelled_dones`], the same pairing the applied-set fold uses,
    /// so a rolled-back unit's service is never spuriously restarted (ADR 0029 §4).
    fn propagate_config(&self, reconcile_id: u64) -> Result<()> {
        let steps = self.planroom.wal_steps_for(reconcile_id)?;
        let cancelled = crate::wal::cancelled_dones(&steps);
        let mut units: Vec<String> = Vec::new();
        for step in &steps {
            if step.state != WalStepState::Done || step.changed != Some(true) {
                continue;
            }
            if cancelled.contains(&step.seq) {
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
        let ord = steps
            .iter()
            .map(|s| s.step_ord)
            .max()
            .map(|m| m + 1)
            .unwrap_or(0);
        // NOTE: same host-root placeholder as `enact` — see the note there.
        let unit_path = [self.host.clone()];
        for (n, unit) in units.into_iter().enumerate() {
            let glyph = Glyph::SystemdService { unit: unit.clone() };
            let cid = scroll_format::content_id_of_glyph(&glyph);
            let op = GlyphOp::Noop {
                cid,
                glyph: glyph.clone(),
            };
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
        self.planroom
            .set_attempt_phase(reconcile_id, AttemptPhase::Committed)?;
        let outcomes = applied_outcomes(&self.planroom.wal_steps()?);
        self.planroom.put_applied_state(&AppliedState {
            scroll_content_id: desired.content_id,
            scroll: desired.scroll.clone(),
            outcomes,
        })?;
        let revision = self
            .planroom
            .revision(
                self.planroom
                    .latest_revision_id()?
                    .expect("a committed attempt projects a revision"),
            )
            .map(|rev| rev.expect("the latest revision id resolves"))?;
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
        let scroll = prior
            .map(|a| a.scroll)
            .unwrap_or_else(|| empty_scroll(&self.host));
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
        self.planroom
            .set_attempt_phase(reconcile_id, AttemptPhase::RollingBack)?;
        loop {
            let steps = self.planroom.wal_steps_for(reconcile_id)?;
            let Some(target) = next_reversible(&steps) else {
                break;
            };
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
                warn!(glyph_key = %target.glyph_key, phase = "reverse", ?e, "rollback step failed");
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

    /// [`Foreman::rollback_attempt`] restricted to one leaf unit's steps (ADR 0029
    /// §4). The LIFO reversal is identical; the `next_reversible` search runs over
    /// only the steps whose `unit_path` equals this unit's, so a sibling unit's
    /// applied glyphs are never in the set and stay committed. This is the
    /// deliberate per-unit `on_exhaust = rollback`; whole-attempt crash recovery
    /// still uses the unscoped [`Foreman::rollback_attempt`].
    fn rollback_unit(&self, reconcile_id: u64, unit_path: &[String]) -> Result<()> {
        loop {
            let steps = self.planroom.wal_steps_for(reconcile_id)?;
            let scoped: Vec<WalStep> = steps
                .into_iter()
                .filter(|s| s.unit_path == unit_path)
                .collect();
            let Some(target) = next_reversible(&scoped).cloned() else {
                break;
            };
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
                warn!(glyph_key = %target.glyph_key, phase = "reverse", ?e, "rollback step failed");
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
        let Some(attempt) = self.planroom.latest_attempt()? else {
            return Ok(());
        };
        if attempt.phase.is_settled() {
            return self.cache_applied_state();
        }
        self.redrive_intended(&attempt)?;
        self.rollback_attempt(attempt.reconcile_id)?;
        self.planroom
            .set_attempt_phase(attempt.reconcile_id, AttemptPhase::RolledBack)?;
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

/// A typed write-path failure the HTTP surface maps to a structured
/// `{ kind, message }` body (ADR 0029 §5). `WalUnreadable` and
/// `ManifestUndecodable` carry actionable messages instead of leaking a raw
/// rusqlite/postcard internal; `Internal` wraps anything else.
#[derive(Debug)]
pub enum ForemanError {
    WalUnreadable { detail: String },
    ManifestUndecodable { detail: String },
    Internal(anyhow::Error),
}

impl ForemanError {
    pub fn kind(&self) -> &'static str {
        match self {
            ForemanError::WalUnreadable { .. } => "wal-unreadable",
            ForemanError::ManifestUndecodable { .. } => "manifest-undecodable",
            ForemanError::Internal(_) => "internal",
        }
    }

    pub fn message(&self) -> String {
        match self {
            ForemanError::WalUnreadable { .. } => {
                "golemd couldn't read its write-ahead log; it may be from an incompatible golemd version. Run `fleet reset` on this host to start from a clean state.".to_string()
            }
            ForemanError::ManifestUndecodable { detail } => {
                format!("golemd couldn't decode the manifest: {detail}")
            }
            ForemanError::Internal(e) => format!("{e:#}"),
        }
    }
}

impl std::fmt::Display for ForemanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for ForemanError {}

/// The `Remove` ops of one or more vanished units that share a surviving-ancestor
/// `unit_path`, enacted together as one unit under that ancestor's `policy_chain`
/// (ADR 0031 §4). The chain borrows from the desired scroll for the duration of
/// the reconcile.
struct RemoveGroup<'a> {
    unit_path: Vec<String>,
    ops: Vec<GlyphOp>,
    policy_chain: Vec<&'a scroll_format::Policy>,
}

/// A leaf unit flattened into a leaf `Scroll` so [`plan`] can diff its glyphs
/// against the whole prior applied set. The diff naturally yields Installs and
/// Replaces for this unit's glyphs; the caller filters out Removes (which the
/// whole-scroll vanished-removes pass owns — Resolution 1).
fn leaf_as_scroll(unit: &LeafUnit<'_>) -> Scroll {
    Scroll {
        name: unit.path.last().cloned().unwrap_or_default(),
        policy: None,
        contents: Contents::Glyphs(unit.glyphs.to_vec()),
    }
}

/// Every node path in the new scroll tree: each leaf unit's path plus all its
/// prefixes (so interior branch and root paths are included). The set the
/// vanished-removes pass matches a recorded path's longest surviving prefix
/// against (ADR 0031 §4).
fn node_paths(desired: &Scroll, units: &[LeafUnit<'_>]) -> Vec<Vec<String>> {
    let mut nodes: Vec<Vec<String>> = vec![vec![desired.name.clone()]];
    for unit in units {
        for len in 1..=unit.path.len() {
            let prefix = unit.path[..len].to_vec();
            if !nodes.contains(&prefix) {
                nodes.push(prefix);
            }
        }
    }
    nodes
}

/// The longest prefix of a recorded `unit_path` that still exists as a node in
/// the new tree — the nearest surviving ancestor (ADR 0031 §4). `None` when the
/// recorded path is empty (a Plan-1 host-root placeholder), so the caller falls
/// back to the host root.
fn surviving_prefix(recorded: &[String], nodes: &[Vec<String>]) -> Option<Vec<String>> {
    if recorded.is_empty() {
        return None;
    }
    for len in (1..=recorded.len()).rev() {
        let prefix = recorded[..len].to_vec();
        if nodes.contains(&prefix) {
            return Some(prefix);
        }
    }
    None
}

/// The ancestor policies along a surviving node path, root-to-leaf — the policy
/// chain that resolves a vanished-removes group's `RetryConfig` (ADR 0031 §4).
fn policy_chain_for_path<'a>(desired: &'a Scroll, path: &[String]) -> Vec<&'a Policy> {
    let mut chain = Vec::new();
    let mut node = desired;
    if path.first().map(String::as_str) != Some(node.name.as_str()) {
        return chain;
    }
    if let Some(p) = &node.policy {
        chain.push(p);
    }
    for name in &path[1..] {
        let Contents::Groups(children) = &node.contents else {
            break;
        };
        let Some(next) = children.iter().find(|c| &c.name == name) else {
            break;
        };
        node = next;
        if let Some(p) = &node.policy {
            chain.push(p);
        }
    }
    chain
}

/// The ops still failing after a unit's round loop, mapped to [`UnitFailure`]s:
/// a `Fatal` op is class `Fatal`; a `Retryable` op that hit a limit is
/// `RetriesExhausted`. `attempts` is the number of rounds the unit ran.
fn unit_failures(
    ops: &[GlyphOp],
    classes: &[StepClass],
    unit_path: &[String],
    rounds: u32,
) -> Vec<UnitFailure> {
    let mut failures = Vec::new();
    for (ord, class) in classes.iter().enumerate() {
        let StepClass::Failed(retry_class, message) = class else {
            continue;
        };
        let (class, attempts) = match retry_class {
            RetryClass::Fatal => (FailClass::Fatal, 1),
            RetryClass::Retryable => (FailClass::RetriesExhausted, rounds),
        };
        failures.push(UnitFailure {
            glyph_key: ops[ord].key(),
            unit_path: unit_path.to_vec(),
            phase: phase_of(&ops[ord]),
            class,
            attempts,
            message: message.clone(),
            rolled_back: false,
        });
    }
    failures
}

fn glyph_action_of(op: &GlyphOp) -> GlyphAction {
    match op {
        GlyphOp::Install { .. } => GlyphAction::Install,
        GlyphOp::Replace { .. } => GlyphAction::Replace,
        GlyphOp::Remove { .. } => GlyphAction::Remove,
        GlyphOp::Noop { .. } => GlyphAction::Noop,
    }
}

/// One [`GlyphLine`] per op in enact order (ADR 0029 addendum): a `Noop` is
/// `Unchanged` with zero attempts; a still-failing op is `Failed` carrying its
/// message and attempts; an op that applied but was undone by this unit's
/// `on_exhaust = rollback` is `RolledBack`; anything else that applied and stayed
/// is `Applied`.
fn glyph_lines(
    ops: &[GlyphOp],
    classes: &[StepClass],
    rolled_back: bool,
    rounds: u32,
) -> Vec<GlyphLine> {
    ops.iter()
        .zip(classes.iter())
        .map(|(op, class)| {
            let action = glyph_action_of(op);
            let (outcome, attempts, message) = match (class, action) {
                (StepClass::Failed(retry_class, msg), _) => {
                    let attempts = match retry_class {
                        RetryClass::Fatal => 1,
                        RetryClass::Retryable => rounds,
                    };
                    (GlyphOutcome::Failed, attempts, Some(msg.clone()))
                }
                (StepClass::Ok, GlyphAction::Noop) => (GlyphOutcome::Unchanged, 0, None),
                (StepClass::Ok, _) if rolled_back => (GlyphOutcome::RolledBack, 1, None),
                (StepClass::Ok, _) => (GlyphOutcome::Applied, 1, None),
            };
            GlyphLine {
                glyph_key: op.key(),
                action,
                outcome,
                attempts,
                message,
            }
        })
        .collect()
}

/// Which side of the bracket a failed op runs on — a `Remove` fails in
/// `Reverse`, everything else in `Enact`. (A reverse-then-apply `Replace` whose
/// reverse leg failed is reported under `Enact`; the distinction the report cares
/// about is remove-teardown vs. forward enact.)
fn phase_of(op: &GlyphOp) -> Phase {
    match op {
        GlyphOp::Remove { .. } => Phase::Reverse,
        _ => Phase::Enact,
    }
}

/// The still-failing retryable ops eligible for the next round: those whose
/// in-memory class is `Failed(Retryable, _)`. A `Fatal` op is terminal and never
/// re-driven; an `Ok` op is done (ADR 0029 §1).
fn remaining_ops(ops: &[GlyphOp], classes: &[StepClass]) -> Vec<u64> {
    let mut remaining = Vec::new();
    for (ord, _op) in ops.iter().enumerate() {
        if let StepClass::Failed(RetryClass::Retryable, _) = classes[ord] {
            remaining.push(ord as u64);
        }
    }
    remaining
}

/// The `class` tag logged and reported for a terminal failure.
fn failclass_tag(class: FailClass) -> &'static str {
    match class {
        FailClass::Fatal => "fatal",
        FailClass::RetriesExhausted => "retries-exhausted",
    }
}

/// Log a `Failed` bracket the moment its row is written (ADR 0029 §2): a
/// retryable at `warn` (it may retry), a fatal at `error` (terminal). Only the
/// glyph key and the reconciler's reason are logged — never contents or secrets.
fn log_step_failure(glyph_key: &str, round: u32, class: &StepClass) {
    match class {
        StepClass::Failed(RetryClass::Retryable, msg) => {
            warn!(glyph_key, round, class = "retryable", reason = %msg, "enact failed; will retry");
        }
        StepClass::Failed(RetryClass::Fatal, msg) => {
            error!(glyph_key, round, class = "fatal", reason = %msg, "enact failed; not retryable");
        }
        StepClass::Ok => {}
    }
}

fn top_outcome_tag(outcome: crate::report::TopOutcome) -> &'static str {
    match outcome {
        crate::report::TopOutcome::Settled => "settled",
        crate::report::TopOutcome::Partial => "partial",
        crate::report::TopOutcome::RolledBack => "rolled_back",
    }
}

/// The closing info line of an apply: the revision this attempt projected, its
/// outcome count, the rolled-up top outcome, and the per-unit fate counts.
fn log_settled(report: &ReconcileReport) {
    let mut settled = 0u32;
    let mut partial = 0u32;
    let mut rolled_back = 0u32;
    for unit in &report.units {
        match unit.outcome {
            UnitOutcome::Settled => settled += 1,
            UnitOutcome::Partial => partial += 1,
            UnitOutcome::RolledBack => rolled_back += 1,
        }
    }
    info!(
        revision = report.revision.id,
        outcomes = report.revision.outcomes.len(),
        outcome = top_outcome_tag(report.outcome),
        units_settled = settled,
        units_partial = partial,
        units_rolled_back = rolled_back,
        "revision recorded"
    );
}

/// Map a foreman-internal [`UnitResult`] onto the wire [`UnitReport`] (ADR 0029
/// §5). The internal outcome, phase, and class enums translate one-for-one.
fn unit_report_from(result: UnitResult) -> UnitReport {
    let failures = result
        .failures
        .into_iter()
        .map(|f| GlyphFailure {
            glyph_key: f.glyph_key,
            unit_path: f.unit_path,
            phase: match f.phase {
                Phase::Enact => FailPhase::Enact,
                Phase::Reverse => FailPhase::Reverse,
            },
            class: match f.class {
                FailClass::Fatal => FailClassReport::Fatal,
                FailClass::RetriesExhausted => FailClassReport::RetriesExhausted,
            },
            attempts: f.attempts,
            message: f.message,
            rolled_back: f.rolled_back,
        })
        .collect();
    UnitReport {
        unit_path: result.unit_path,
        outcome: result.outcome,
        glyphs: result.glyphs,
        failures,
    }
}

// A leaf scroll (ADR 0031 §1) holding no glyphs — the desired state that diffs
// every currently-applied glyph to a `Remove`, used when this host has no scroll
// in the manifest.
fn empty_scroll(host: &str) -> Scroll {
    Scroll {
        name: host.to_string(),
        policy: None,
        contents: scroll_format::Contents::Glyphs(vec![]),
    }
}

/// Whether a `Replace` on this glyph can update in place — one `Apply` whose
/// captured inverse restores the prior version — instead of reverse-then-apply
/// (ADR 0020 §4). Only `Filesystem::File`: an atomic overwrite has no window
/// where the file is absent and stays exactly reversible. `aptPackage` and
/// `systemdService` have distinct reverse/re-apply effects (a package
/// remove/install, a unit stop/start) and must stay reverse-then-apply.
fn replaces_in_place(glyph: &Glyph) -> bool {
    matches!(
        glyph,
        Glyph::Filesystem {
            entry: Entry::File { .. },
            ..
        }
    )
}

fn applied_cid_of(op: &GlyphOp, action: WalAction) -> ContentId {
    match op {
        GlyphOp::Install { cid, .. } | GlyphOp::Noop { cid, .. } | GlyphOp::Remove { cid, .. } => {
            *cid
        }
        GlyphOp::Replace {
            new_cid, old_cid, ..
        } => match action {
            WalAction::Apply => *new_cid,
            WalAction::Reverse => *old_cid,
        },
    }
}

/// Whether an `Intended` step has any later terminal row (`Done`/`Failed`/
/// `Reversed`) for the same step, matched by `step_ord`+`action`: a step's rows
/// share those, and `step_ord` is attempt-unique (see `enact_unit`) so the match
/// never crosses units. An `Intended` with no terminal successor is what
/// [`Foreman::redrive_intended`] must re-drive.
fn has_terminal(steps: &[WalStep], intended: &WalStep) -> bool {
    steps.iter().any(|s| {
        s.seq > intended.seq
            && s.step_ord == intended.step_ord
            && s.action == intended.action
            && matches!(
                s.state,
                WalStepState::Done | WalStepState::Failed | WalStepState::Reversed
            )
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

/// Fold the per-scroll policy cascade over the fleet default to get one leaf
/// unit's effective `RetryConfig`. Cascade order, NEAREST WINS: `golemd.toml`
/// default → ancestor branch policies root→leaf → the leaf's own policy. A field
/// unset at every scope stays at the config default (which itself falls back to
/// the built-in). `policy_chain` is root-most first, so left-to-right folding
/// lets the nearest (leaf) scope win. See ADR 0029 §3 and ADR 0031 §3.
pub(crate) fn resolve_retry(
    base: &RetryConfig,
    policy_chain: &[&scroll_format::Policy],
) -> RetryConfig {
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
        Glyph::Filesystem {
            path,
            entry: Entry::File { .. },
        } => Some(path.clone()),
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
    use crate::reconciler::EnactResult;
    use crate::report::{FailClassReport, TopOutcome, UnitOutcome};
    use scroll_format::{Glyph, Manifest, OnExhaust, Policy};
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct Recorder {
        calls: Mutex<Vec<String>>,
        present: Mutex<BTreeMap<String, ContentId>>,
    }
    impl Recorder {
        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
    }
    impl Reconciler for Recorder {
        fn apply(&self, glyph: &Glyph, cid: ContentId) -> EnactResult<Outcome> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("apply {}", glyph.key()));
            self.present.lock().unwrap().insert(glyph.key(), cid);
            Ok(Outcome {
                op: GlyphOp::Install {
                    cid,
                    glyph: glyph.clone(),
                },
                cid,
                inverse: crate::reconciler::inverse_of(glyph),
                changed: true,
            })
        }
        fn reverse(&self, outcome: &Outcome) -> EnactResult<()> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("reverse {}", outcome.op.key()));
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
            Self {
                fails_left: Mutex::new(fails),
                calls: Mutex::new(0),
            }
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
                    op: GlyphOp::Install {
                        cid,
                        glyph: glyph.clone(),
                    },
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
            Self {
                make,
                calls: Mutex::new(0),
            }
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

    /// A fake reconciler scripted per glyph key: a key can be told to fail `Fatal`
    /// or `Retryable` a given number of times (or always); everything else applies
    /// and reverses like [`Recorder`], tracking the present set so a rollback is
    /// observable. Models the existing fakes, keyed so best-effort and per-unit
    /// isolation can be exercised (ADR 0029 §1/§4).
    struct ScriptedReconciler {
        present: Mutex<BTreeMap<String, ContentId>>,
        fatal: Mutex<Vec<String>>,
        retryable_left: Mutex<BTreeMap<String, u32>>,
        retryable_always: Mutex<Vec<String>>,
        fatal_reverse: Mutex<Vec<String>>,
        restarts: Mutex<Vec<String>>,
    }
    impl ScriptedReconciler {
        fn new() -> Self {
            Self {
                present: Mutex::new(BTreeMap::new()),
                fatal: Mutex::new(Vec::new()),
                retryable_left: Mutex::new(BTreeMap::new()),
                retryable_always: Mutex::new(Vec::new()),
                fatal_reverse: Mutex::new(Vec::new()),
                restarts: Mutex::new(Vec::new()),
            }
        }
        fn fatal_reverse_on(self, key: &str) -> Self {
            self.fatal_reverse.lock().unwrap().push(key.into());
            self
        }
        fn fatal_on(self, key: &str) -> Self {
            self.fatal.lock().unwrap().push(key.into());
            self
        }
        fn retryable_times(self, key: &str, times: u32) -> Self {
            self.retryable_left
                .lock()
                .unwrap()
                .insert(key.into(), times);
            self
        }
        fn retryable_always(self, key: &str) -> Self {
            self.retryable_always.lock().unwrap().push(key.into());
            self
        }
        fn ok_default(self) -> Self {
            self
        }
        fn present_keys(&self) -> Vec<String> {
            self.present.lock().unwrap().keys().cloned().collect()
        }
        fn restarts(&self) -> Vec<String> {
            self.restarts.lock().unwrap().clone()
        }
    }
    impl Reconciler for ScriptedReconciler {
        fn apply(&self, glyph: &Glyph, cid: ContentId) -> EnactResult<Outcome> {
            let key = glyph.key();
            if self.fatal.lock().unwrap().iter().any(|k| k == &key) {
                return Err(EnactError::Fatal(format!("scripted fatal for {key}")));
            }
            if self
                .retryable_always
                .lock()
                .unwrap()
                .iter()
                .any(|k| k == &key)
            {
                return Err(EnactError::Retryable(format!(
                    "scripted retryable for {key}"
                )));
            }
            {
                let mut left = self.retryable_left.lock().unwrap();
                if let Some(n) = left.get_mut(&key) {
                    if *n > 0 {
                        *n -= 1;
                        return Err(EnactError::Retryable(format!(
                            "scripted retryable for {key}"
                        )));
                    }
                }
            }
            self.present.lock().unwrap().insert(key.clone(), cid);
            Ok(Outcome {
                op: GlyphOp::Install {
                    cid,
                    glyph: glyph.clone(),
                },
                cid,
                inverse: crate::reconciler::inverse_of(glyph),
                changed: true,
            })
        }
        fn reverse(&self, outcome: &Outcome) -> EnactResult<()> {
            let key = outcome.op.key();
            if self.fatal_reverse.lock().unwrap().iter().any(|k| k == &key) {
                return Err(EnactError::Fatal(format!(
                    "scripted fatal reverse for {key}"
                )));
            }
            self.present.lock().unwrap().remove(&key);
            Ok(())
        }
        fn restart_unit(&self, unit: &str) -> EnactResult<()> {
            self.restarts.lock().unwrap().push(unit.to_string());
            Ok(())
        }
    }

    fn retry_config(max_attempts: u32) -> RetryConfig {
        RetryConfig {
            max_attempts,
            base_delay_ms: 0,
            ..Default::default()
        }
    }

    fn foreman(host: &str, reconciler: Box<dyn Reconciler>) -> Foreman {
        Foreman::new(host.into(), Box::new(MemoryPlanRoom::new()), reconciler)
            .with_retry_config(retry_config(3))
    }

    /// A foreman over the scripted reconciler, on host `"host"`, tight retries so
    /// tests run fast. Kept behind an `Arc` so the test can read the reconciler's
    /// present set after an apply.
    fn foreman_with(reconciler: ScriptedReconciler) -> ScriptedForeman {
        let rec = Arc::new(reconciler);
        let f = Foreman::new(
            "host".into(),
            Box::new(MemoryPlanRoom::new()),
            Box::new(rec.clone()),
        )
        .with_retry_config(RetryConfig {
            max_attempts: 3,
            base_delay_ms: 0,
            ..Default::default()
        });
        ScriptedForeman { foreman: f, rec }
    }

    struct ScriptedForeman {
        foreman: Foreman,
        rec: Arc<ScriptedReconciler>,
    }
    impl ScriptedForeman {
        fn with_retry_config(mut self, cfg: RetryConfig) -> Self {
            self.foreman = self.foreman.with_retry_config(cfg);
            self
        }
        fn apply_scroll(&self, mut scroll: Scroll) -> Result<ReconcileReport, ForemanError> {
            scroll.name = self.foreman.host().to_string();
            let bytes = scroll_format::to_bytes(&Manifest::from_scrolls(vec![scroll], "test"));
            self.foreman.apply_manifest(&bytes)
        }
    }

    fn applied_keys(f: &ScriptedForeman) -> Vec<String> {
        f.rec.present_keys()
    }

    fn apt(name: &str) -> Glyph {
        Glyph::AptPackage { name: name.into() }
    }

    fn unit_file(path: &str, contents: &str) -> Glyph {
        Glyph::Filesystem {
            path: path.into(),
            entry: Entry::File {
                contents: contents.into(),
                perms: scroll_format::Perms {
                    mode: 0o644,
                    owner: None,
                    group: None,
                },
            },
        }
    }

    fn manifest(scrolls: Vec<Scroll>) -> Vec<u8> {
        scroll_format::to_bytes(&Manifest::from_scrolls(scrolls, "test"))
    }

    fn scroll(host: &str, glyphs: Vec<Glyph>) -> Scroll {
        Scroll {
            name: host.into(),
            policy: None,
            contents: Contents::Glyphs(glyphs),
        }
    }

    fn leaf_scroll(name: &str, glyphs: Vec<Glyph>) -> Scroll {
        Scroll {
            name: name.into(),
            policy: None,
            contents: Contents::Glyphs(glyphs),
        }
    }

    fn leaf_scroll_with_policy(name: &str, policy: Policy, glyphs: Vec<Glyph>) -> Scroll {
        Scroll {
            name: name.into(),
            policy: Some(policy),
            contents: Contents::Glyphs(glyphs),
        }
    }

    fn branch_scroll(name: &str, groups: Vec<Scroll>) -> Scroll {
        Scroll {
            name: name.into(),
            policy: None,
            contents: Contents::Groups(groups),
        }
    }

    // --- Task 4: best-effort within a unit; sibling isolation ---

    #[test]
    fn a_fatal_glyph_does_not_veto_the_rest_of_its_unit() {
        let reconciler = ScriptedReconciler::new().fatal_on("apt:bad").ok_default();
        let foreman = foreman_with(reconciler).with_retry_config(RetryConfig {
            on_exhaust: OnExhaustConfig::Keep,
            base_delay_ms: 0,
            ..Default::default()
        });
        let scroll = leaf_scroll("unit", vec![apt("bad"), apt("good")]);
        let report = foreman.apply_scroll(scroll).unwrap();
        assert_eq!(report.units.len(), 1);
        assert_eq!(report.units[0].failures.len(), 1);
        assert_eq!(report.units[0].failures[0].glyph_key, "apt:bad");
        assert!(applied_keys(&foreman).contains(&"apt:good".to_string()));
    }

    #[test]
    fn one_unit_failing_leaves_a_sibling_unit_settled() {
        let reconciler = ScriptedReconciler::new().fatal_on("apt:bad").ok_default();
        let foreman = foreman_with(reconciler);
        let scroll = branch_scroll(
            "host",
            vec![
                leaf_scroll("broken", vec![apt("bad")]),
                leaf_scroll("healthy", vec![apt("good")]),
            ],
        );
        let report = foreman.apply_scroll(scroll).unwrap();
        let healthy = report
            .units
            .iter()
            .find(|u| u.unit_path.last().unwrap() == "healthy")
            .unwrap();
        assert!(healthy.failures.is_empty());
        assert!(applied_keys(&foreman).contains(&"apt:good".to_string()));
    }

    // --- Task 5: backoff/jitter/dual limits ---

    #[test]
    fn a_retryable_glyph_succeeds_within_the_attempt_limit() {
        let reconciler = ScriptedReconciler::new()
            .retryable_times("apt:flaky", 2)
            .ok_default();
        let foreman = foreman_with(reconciler).with_retry_config(RetryConfig {
            max_attempts: 5,
            base_delay_ms: 1,
            max_delay_ms: 2,
            jitter_fraction: 0.0,
            ..Default::default()
        });
        let report = foreman
            .apply_scroll(leaf_scroll("u", vec![apt("flaky")]))
            .unwrap();
        assert!(report.units[0].failures.is_empty());
        assert!(applied_keys(&foreman).contains(&"apt:flaky".to_string()));
    }

    #[test]
    fn round_delay_saturates_at_max_delay() {
        let cfg = RetryConfig {
            base_delay_ms: 100,
            backoff_multiplier: 10.0,
            max_delay_ms: 500,
            jitter_fraction: 0.0,
            ..Default::default()
        };
        assert_eq!(super::round_delay(&cfg, 1).as_millis(), 100);
        assert_eq!(super::round_delay(&cfg, 2).as_millis(), 500);
        assert_eq!(super::round_delay(&cfg, 5).as_millis(), 500);
    }

    #[test]
    fn a_never_succeeding_retryable_gives_up_as_retries_exhausted() {
        let reconciler = ScriptedReconciler::new()
            .retryable_always("apt:doomed")
            .ok_default();
        let foreman = foreman_with(reconciler).with_retry_config(RetryConfig {
            max_attempts: 3,
            base_delay_ms: 1,
            max_delay_ms: 1,
            jitter_fraction: 0.0,
            max_elapsed_ms: 60_000,
            on_exhaust: OnExhaustConfig::Keep,
            ..Default::default()
        });
        let report = foreman
            .apply_scroll(leaf_scroll("u", vec![apt("doomed")]))
            .unwrap();
        assert_eq!(report.units[0].failures.len(), 1);
        assert_eq!(
            report.units[0].failures[0].class,
            FailClassReport::RetriesExhausted
        );
        assert_eq!(report.units[0].failures[0].attempts, 3);
    }

    // --- Task 6: per-unit on_exhaust, scoped rollback ---

    #[test]
    fn a_units_rollback_undoes_only_its_own_glyphs() {
        let reconciler = ScriptedReconciler::new().fatal_on("apt:bad").ok_default();
        let foreman = foreman_with(reconciler);
        let scroll = branch_scroll(
            "host",
            vec![
                leaf_scroll("broken", vec![apt("good-in-broken"), apt("bad")]),
                leaf_scroll("healthy", vec![apt("healthy-pkg")]),
            ],
        );
        let report = foreman.apply_scroll(scroll).unwrap();
        let broken = report
            .units
            .iter()
            .find(|u| u.unit_path.last().unwrap() == "broken")
            .unwrap();
        assert_eq!(broken.outcome, UnitOutcome::RolledBack);
        assert!(!applied_keys(&foreman).contains(&"apt:good-in-broken".to_string()));
        assert!(applied_keys(&foreman).contains(&"apt:healthy-pkg".to_string()));
    }

    #[test]
    fn a_keep_unit_leaves_its_applied_glyphs() {
        let reconciler = ScriptedReconciler::new().fatal_on("apt:bad").ok_default();
        let foreman = foreman_with(reconciler);
        let leaf = leaf_scroll_with_policy(
            "u",
            Policy {
                on_exhaust: Some(OnExhaust::Keep),
                ..Default::default()
            },
            vec![apt("kept"), apt("bad")],
        );
        let report = foreman
            .apply_scroll(branch_scroll("host", vec![leaf]))
            .unwrap();
        assert_eq!(report.units[0].outcome, UnitOutcome::Partial);
        assert!(applied_keys(&foreman).contains(&"apt:kept".to_string()));
        assert!(report.units[0].failures.iter().all(|f| !f.rolled_back));
    }

    // --- Task 8: report shape, typed errors, sibling-remove regression ---

    #[test]
    fn a_rolled_back_unit_reports_glyph_lines_in_enact_order() {
        use crate::report::{GlyphAction, GlyphOutcome};
        let reconciler = ScriptedReconciler::new()
            .retryable_always("systemd:fishnet.service")
            .ok_default();
        let foreman = foreman_with(reconciler).with_retry_config(RetryConfig {
            max_attempts: 5,
            base_delay_ms: 0,
            max_delay_ms: 0,
            jitter_fraction: 0.0,
            on_exhaust: OnExhaustConfig::Rollback,
            ..Default::default()
        });
        let leaf = leaf_scroll(
            "unit",
            vec![
                apt("podman"),
                unit_file("/etc/containers/systemd/fishnet.container", "v1"),
                Glyph::SystemdService {
                    unit: "fishnet.service".into(),
                },
            ],
        );
        let report = foreman
            .apply_scroll(branch_scroll("host", vec![leaf]))
            .unwrap();
        let unit = report
            .units
            .iter()
            .find(|u| u.unit_path.last().unwrap() == "unit")
            .unwrap();
        assert_eq!(unit.outcome, UnitOutcome::RolledBack);
        let outcomes: Vec<GlyphOutcome> = unit.glyphs.iter().map(|g| g.outcome).collect();
        assert_eq!(
            outcomes,
            vec![
                GlyphOutcome::RolledBack,
                GlyphOutcome::RolledBack,
                GlyphOutcome::Failed
            ]
        );
        let actions: Vec<GlyphAction> = unit.glyphs.iter().map(|g| g.action).collect();
        assert_eq!(
            actions,
            vec![
                GlyphAction::Install,
                GlyphAction::Install,
                GlyphAction::Install
            ]
        );
        let failed = unit.glyphs.iter().find(|g| g.outcome == GlyphOutcome::Failed).unwrap();
        assert_eq!(failed.glyph_key, "systemd:fishnet.service");
        assert_eq!(failed.attempts, 5);
        assert_eq!(failed.message.as_deref(), Some("scripted retryable for systemd:fishnet.service"));
    }

    #[test]
    fn a_noop_reconcile_reports_all_unchanged_glyph_lines() {
        use crate::report::GlyphOutcome;
        let reconciler = ScriptedReconciler::new().ok_default();
        let foreman = foreman_with(reconciler);
        let scroll = leaf_scroll("host", vec![apt("podman"), apt("fishnet")]);
        foreman.apply_scroll(scroll.clone()).unwrap();
        let report = foreman.apply_scroll(scroll).unwrap();
        assert_eq!(report.units.len(), 1);
        assert_eq!(report.units[0].outcome, UnitOutcome::Settled);
        assert!(!report.units[0].glyphs.is_empty());
        assert!(report
            .units[0]
            .glyphs
            .iter()
            .all(|g| g.outcome == GlyphOutcome::Unchanged && g.attempts == 0));
    }

    #[test]
    fn apply_manifest_returns_a_report_with_units_in_source_order() {
        let reconciler = ScriptedReconciler::new().ok_default();
        let foreman = foreman_with(reconciler);
        let scroll = branch_scroll(
            "host",
            vec![
                leaf_scroll("a", vec![apt("one")]),
                leaf_scroll("b", vec![apt("two")]),
            ],
        );
        let report = foreman.apply_scroll(scroll).unwrap();
        assert_eq!(report.outcome, TopOutcome::Settled);
        let names: Vec<String> = report
            .units
            .iter()
            .map(|u| u.unit_path.last().unwrap().clone())
            .collect();
        assert_eq!(names, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn an_undecodable_manifest_is_a_typed_error() {
        let foreman = foreman_with(ScriptedReconciler::new().ok_default());
        match foreman.foreman.apply_manifest(b"not a manifest") {
            Err(e) => assert_eq!(e.kind(), "manifest-undecodable"),
            Ok(_) => panic!("expected a typed error"),
        }
    }

    #[test]
    fn enacting_one_unit_does_not_remove_a_sibling_units_applied_glyph() {
        let reconciler = ScriptedReconciler::new().ok_default();
        let foreman = foreman_with(reconciler);
        let scroll = branch_scroll(
            "host",
            vec![
                leaf_scroll("a", vec![apt("a-pkg")]),
                leaf_scroll("b", vec![apt("b-pkg")]),
            ],
        );
        foreman.apply_scroll(scroll.clone()).unwrap();
        assert!(applied_keys(&foreman).contains(&"apt:b-pkg".to_string()));

        let report = foreman.apply_scroll(scroll).unwrap();
        assert_eq!(report.outcome, TopOutcome::Settled);
        assert!(applied_keys(&foreman).contains(&"apt:b-pkg".to_string()));
        let attempt = report.revision.id;
        let _ = attempt;
        for unit in &report.units {
            assert!(unit.failures.is_empty());
        }
        assert!(applied_keys(&foreman).contains(&"apt:a-pkg".to_string()));
    }

    #[test]
    fn a_vanished_unit_glyph_is_removed_under_a_surviving_ancestor() {
        let reconciler = ScriptedReconciler::new().ok_default();
        let foreman = foreman_with(reconciler);
        let before = branch_scroll(
            "host",
            vec![
                leaf_scroll("keep", vec![apt("stays")]),
                leaf_scroll("gone", vec![apt("removed")]),
            ],
        );
        foreman.apply_scroll(before).unwrap();
        assert!(applied_keys(&foreman).contains(&"apt:removed".to_string()));

        let after = branch_scroll("host", vec![leaf_scroll("keep", vec![apt("stays")])]);
        let report = foreman.apply_scroll(after).unwrap();
        assert_eq!(report.outcome, TopOutcome::Settled);
        assert!(!applied_keys(&foreman).contains(&"apt:removed".to_string()));
        assert!(applied_keys(&foreman).contains(&"apt:stays".to_string()));
    }

    // --- Review fixes: removes-group path isolation, whole-reconcile budget,
    //     rollback-aware config propagation ---

    #[test]
    fn a_flat_host_removes_rollback_does_not_reverse_a_present_glyph() {
        let reconciler = ScriptedReconciler::new()
            .fatal_reverse_on("apt:goes")
            .ok_default();
        let foreman = foreman_with(reconciler).with_retry_config(RetryConfig {
            max_attempts: 1,
            base_delay_ms: 0,
            ..Default::default()
        });
        foreman
            .apply_scroll(leaf_scroll("host", vec![apt("goes")]))
            .unwrap();
        assert!(applied_keys(&foreman).contains(&"apt:goes".to_string()));

        let report = foreman
            .apply_scroll(leaf_scroll("host", vec![apt("stays")]))
            .unwrap();
        let removes = report
            .units
            .iter()
            .find(|u| u.unit_path.last().map(String::as_str) == Some("<removes>"))
            .expect("the vanished-removes group reports its own unit");
        assert_eq!(removes.outcome, UnitOutcome::RolledBack);
        assert!(
            applied_keys(&foreman).contains(&"apt:stays".to_string()),
            "a failing vanished-remove rolling back must not reverse the present unit's freshly-applied glyph"
        );
    }

    #[test]
    fn a_dropped_glyph_removes_rollback_does_not_reverse_the_surviving_unit() {
        let reconciler = ScriptedReconciler::new()
            .fatal_reverse_on("apt:dropped")
            .ok_default();
        let foreman = foreman_with(reconciler).with_retry_config(RetryConfig {
            max_attempts: 1,
            base_delay_ms: 0,
            ..Default::default()
        });
        let kept = "file:/etc/app/kept.conf".to_string();
        let before = branch_scroll(
            "host",
            vec![leaf_scroll(
                "b",
                vec![unit_file("/etc/app/kept.conf", "v1"), apt("dropped")],
            )],
        );
        foreman.apply_scroll(before).unwrap();
        assert!(applied_keys(&foreman).contains(&kept));

        let after = branch_scroll(
            "host",
            vec![leaf_scroll(
                "b",
                vec![unit_file("/etc/app/kept.conf", "v2")],
            )],
        );
        foreman.apply_scroll(after).unwrap();
        assert!(
            applied_keys(&foreman).contains(&kept),
            "a dropped glyph's remove rolling back must not reverse unit b's replaced glyph"
        );
    }

    #[test]
    fn max_elapsed_bounds_the_whole_reconcile_not_each_unit() {
        let reconciler = ScriptedReconciler::new()
            .retryable_always("apt:one")
            .retryable_always("apt:two")
            .ok_default();
        let foreman = foreman_with(reconciler).with_retry_config(RetryConfig {
            max_attempts: 5,
            base_delay_ms: 5,
            backoff_multiplier: 1.0,
            max_delay_ms: 5,
            jitter_fraction: 0.0,
            max_elapsed_ms: 1,
            on_exhaust: OnExhaustConfig::Keep,
        });
        let scroll = branch_scroll(
            "host",
            vec![
                leaf_scroll("first", vec![apt("one")]),
                leaf_scroll("second", vec![apt("two")]),
            ],
        );
        let report = foreman.apply_scroll(scroll).unwrap();
        let first = report
            .units
            .iter()
            .find(|u| u.unit_path.last().unwrap() == "first")
            .unwrap();
        let second = report
            .units
            .iter()
            .find(|u| u.unit_path.last().unwrap() == "second")
            .unwrap();
        assert_eq!(first.failures[0].class, FailClassReport::RetriesExhausted);
        assert_eq!(second.failures[0].class, FailClassReport::RetriesExhausted);
        assert_eq!(
            second.failures[0].attempts, 1,
            "the budget spent on the first unit leaves the second only its opening round"
        );
    }

    #[test]
    fn a_rolled_back_config_file_is_not_restarted_on_settle() {
        let reconciler = ScriptedReconciler::new().fatal_on("apt:bad").ok_default();
        let foreman = foreman_with(reconciler);
        let leaf = leaf_scroll(
            "unit",
            vec![
                unit_file("/etc/containers/systemd/registry.container", "v1"),
                apt("bad"),
            ],
        );
        let report = foreman
            .apply_scroll(branch_scroll("host", vec![leaf]))
            .unwrap();
        assert_eq!(report.units[0].outcome, UnitOutcome::RolledBack);
        assert!(
            foreman.rec.restarts().is_empty(),
            "a unit whose config rolled back must not restart its service"
        );
    }

    // --- existing contract tests (updated to per-unit best-effort) ---

    #[test]
    fn with_retry_config_is_stored() {
        let foreman = Foreman::new(
            "h".into(),
            Box::new(MemoryPlanRoom::new()),
            Box::new(Recorder::default()),
        )
        .with_retry_config(RetryConfig {
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
        let report = f.apply_manifest(&bytes).unwrap();

        assert_eq!(report.revision.kind, RevisionKind::Reconcile);
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
        f.apply_manifest(&manifest(vec![scroll("h1", vec![apt("nginx"), apt("pg")])]))
            .unwrap();
        rec.calls.lock().unwrap().clear();
        f.apply_manifest(&manifest(vec![scroll("h1", vec![apt("nginx")])]))
            .unwrap();
        assert!(rec.calls().contains(&"reverse apt:pg".to_string()));
    }

    #[test]
    fn empty_scroll_removes_everything() {
        let rec = Arc::new(Recorder::default());
        let f = foreman("h1", Box::new(rec.clone()));
        f.apply_manifest(&manifest(vec![scroll("h1", vec![apt("nginx")])]))
            .unwrap();
        rec.calls.lock().unwrap().clear();
        f.apply_manifest(&manifest(vec![scroll("h1", vec![])]))
            .unwrap();
        assert_eq!(rec.calls(), vec!["reverse apt:nginx"]);
        assert!(f.applied_state().unwrap().unwrap().outcomes.is_empty());
    }

    #[test]
    fn retryable_failures_are_retried_until_success() {
        let flaky = Arc::new(FlakyThenOk::new(2));
        let f = foreman("h1", Box::new(flaky.clone()));
        f.apply_manifest(&manifest(vec![scroll("h1", vec![apt("app")])]))
            .unwrap();
        assert_eq!(*flaky.calls.lock().unwrap(), 3);
    }

    #[test]
    fn no_retry_config_attempts_once() {
        let failing = Arc::new(Failing::new(EnactError::Retryable));
        let f = Foreman::new(
            "h1".into(),
            Box::new(MemoryPlanRoom::new()),
            Box::new(failing.clone()),
        )
        .with_retry_config(retry_config(1));
        let report = f
            .apply_manifest(&manifest(vec![scroll("h1", vec![apt("app")])]))
            .unwrap();
        assert_eq!(report.outcome, TopOutcome::RolledBack);
        assert_eq!(failing.calls(), 1);
    }

    #[test]
    fn exhausted_retries_report_and_roll_back_by_default() {
        let failing = Arc::new(Failing::new(EnactError::Retryable));
        let f = foreman("h1", Box::new(failing.clone()));
        let report = f
            .apply_manifest(&manifest(vec![scroll("h1", vec![apt("app")])]))
            .unwrap();
        assert_eq!(report.outcome, TopOutcome::RolledBack);
        assert_eq!(
            report.units[0].failures[0].class,
            FailClassReport::RetriesExhausted
        );
        assert_eq!(failing.calls(), 3);
        assert!(f.applied_state().unwrap().unwrap().outcomes.is_empty());
        assert_eq!(f.revisions().unwrap().len(), 2);
    }

    #[test]
    fn fatal_failure_is_not_retried_and_rolls_back_by_default() {
        let failing = Arc::new(Failing::new(EnactError::Fatal));
        let f = foreman("h1", Box::new(failing.clone()));
        let report = f
            .apply_manifest(&manifest(vec![scroll("h1", vec![apt("app")])]))
            .unwrap();
        assert_eq!(report.outcome, TopOutcome::RolledBack);
        assert_eq!(report.units[0].failures[0].class, FailClassReport::Fatal);
        assert_eq!(failing.calls(), 1);
        assert!(f.applied_state().unwrap().unwrap().outcomes.is_empty());
        assert_eq!(f.revisions().unwrap().len(), 2);
    }

    struct HostModel {
        present: Mutex<BTreeMap<String, ContentId>>,
        calls: Mutex<Vec<String>>,
    }
    impl HostModel {
        fn new() -> Self {
            Self {
                present: Mutex::new(BTreeMap::new()),
                calls: Mutex::new(vec![]),
            }
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
                op: GlyphOp::Install {
                    cid,
                    glyph: glyph.clone(),
                },
                cid,
                inverse: if already {
                    Inverse::Nothing
                } else {
                    crate::reconciler::inverse_of(glyph)
                },
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
            self.calls
                .lock()
                .unwrap()
                .push(format!("reverse {}", outcome.op.key()));
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
        let nginx = stored
            .outcomes
            .iter()
            .find(|o| o.op.key() == "apt:nginx")
            .unwrap();
        assert_eq!(
            nginx.inverse,
            Inverse::RemoveAptPackage {
                name: "nginx".into()
            },
            "re-apply must not overwrite the real inverse with Nothing"
        );

        f.apply_manifest(&manifest(vec![scroll("h1", vec![])]))
            .unwrap();

        assert!(
            host.present_keys().is_empty(),
            "removal must revert the host"
        );
        assert!(host
            .calls
            .lock()
            .unwrap()
            .contains(&"reverse apt:nginx".to_string()));
        assert!(f.applied_state().unwrap().unwrap().outcomes.is_empty());
    }

    #[test]
    fn resolve_retry_uses_config_when_no_policy() {
        let base = RetryConfig {
            max_attempts: 5,
            ..Default::default()
        };
        let eff = super::resolve_retry(&base, &[]);
        assert_eq!(eff.max_attempts, 5);
        assert_eq!(eff.on_exhaust, OnExhaustConfig::Rollback);
    }

    #[test]
    fn resolve_retry_leaf_overrides_ancestor_overrides_config() {
        let base = RetryConfig {
            max_attempts: 5,
            ..Default::default()
        };
        let ancestor = Policy {
            max_attempts: Some(8),
            on_exhaust: Some(OnExhaust::Rollback),
            ..Policy::default()
        };
        let leaf = Policy {
            on_exhaust: Some(OnExhaust::Keep),
            ..Policy::default()
        };
        let eff = super::resolve_retry(&base, &[&ancestor, &leaf]);
        assert_eq!(eff.max_attempts, 8);
        assert_eq!(eff.on_exhaust, OnExhaustConfig::Keep);
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
                    op: GlyphOp::Install {
                        cid,
                        glyph: glyph.clone(),
                    },
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
        let rec = Arc::new(FailSecond {
            calls: Mutex::new(0),
            reversed: Mutex::new(vec![]),
        });
        let f = foreman("h1", Box::new(rec.clone()));
        let report = f
            .apply_manifest(&manifest(vec![scroll("h1", vec![apt("a"), apt("b")])]))
            .unwrap();
        assert_eq!(report.outcome, TopOutcome::RolledBack);
        assert_eq!(*rec.reversed.lock().unwrap(), vec!["apt:a".to_string()]);
        assert!(f.applied_state().unwrap().unwrap().outcomes.is_empty());
    }
}
