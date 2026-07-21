use anyhow::{bail, Result};
use scroll_format::{from_bytes, AddressedScroll, ContentId, Entry, Glyph, Scroll};
use std::sync::Mutex;
use std::time::Duration;
use tracing::warn;

use crate::journal::{
    AppliedState, AttemptPhase, GlyphOp, Inverse, Outcome, ReconcileAttempt, Revision, RevisionKind,
    WalAction, WalStep, WalStepState,
};
use crate::planroom::PlanRoom;
use crate::reconcile::plan;
use crate::reconciler::{EnactError, EnactResult, Reconciler};
use crate::wal::applied_outcomes;

pub struct Foreman {
    host: String,
    planroom: Box<dyn PlanRoom>,
    reconciler: Box<dyn Reconciler>,
    max_attempts: u32,
    retry_delay: Duration,
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
            max_attempts: 5,
            retry_delay: Duration::from_millis(200),
            write: Mutex::new(()),
        };
        if let Err(e) = foreman.recover() {
            warn!(?e, "startup recovery failed");
        }
        foreman
    }

    pub fn with_retry(mut self, max_attempts: u32, retry_delay: Duration) -> Self {
        self.max_attempts = max_attempts;
        self.retry_delay = retry_delay;
        self
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn apply_manifest(&self, bytes: &[u8]) -> Result<Revision> {
        let manifest = from_bytes(bytes).map_err(|e| anyhow::anyhow!("{e}"))?;
        let selected = self.select(&manifest.scrolls);
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

    fn enact(&self, reconcile_id: u64, ops: &[GlyphOp], prior: &[Outcome]) -> Result<()> {
        for (ord, op) in ops.iter().enumerate() {
            let ord = ord as u64;
            match op {
                GlyphOp::Noop { .. } => {}
                GlyphOp::Install { cid, glyph } => {
                    self.enact_apply(reconcile_id, ord, op, glyph, *cid, None)?;
                }
                GlyphOp::Replace { old_cid, new_cid, glyph } => {
                    if replaces_in_place(glyph) {
                        self.enact_apply(reconcile_id, ord, op, glyph, *new_cid, None)?;
                    } else {
                        let prior_outcome = self.prior_outcome(prior, &op.key(), *old_cid, glyph);
                        self.enact_reverse(reconcile_id, ord, op, &prior_outcome)?;
                        self.enact_apply(reconcile_id, ord, op, glyph, *new_cid, None)?;
                    }
                }
                GlyphOp::Remove { cid, glyph } => {
                    let prior_outcome = self.prior_outcome(prior, &op.key(), *cid, glyph);
                    self.enact_reverse(reconcile_id, ord, op, &prior_outcome)?;
                }
            }
        }
        Ok(())
    }

    fn enact_apply(
        &self,
        reconcile_id: u64,
        ord: u64,
        op: &GlyphOp,
        glyph: &Glyph,
        cid: ContentId,
        intended_inverse: Option<&Inverse>,
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
                )?;
                Err(e)
            }
        }
    }

    fn enact_reverse(
        &self,
        reconcile_id: u64,
        ord: u64,
        op: &GlyphOp,
        prior_outcome: &Outcome,
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
                )?;
                Err(e)
            }
        }
    }

    fn prior_outcome(&self, prior: &[Outcome], key: &str, cid: ContentId, glyph: &Glyph) -> Outcome {
        prior.iter().find(|o| o.op.key() == key).cloned().unwrap_or(Outcome {
            op: GlyphOp::Install { cid, glyph: glyph.clone() },
            cid,
            inverse: crate::reconciler::inverse_of(glyph),
            changed: true,
        })
    }

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
            )?;
        }
        Ok(())
    }

    fn settle(&self, reconcile_id: u64, desired: &SelectedScroll) -> Result<Revision> {
        self.planroom.set_attempt_phase(reconcile_id, AttemptPhase::Committed)?;
        let outcomes = applied_outcomes(&self.planroom.wal_steps()?);
        self.planroom.put_applied_state(&AppliedState {
            scroll_content_id: desired.content_id,
            scroll: desired.scroll.clone(),
            outcomes: outcomes.clone(),
        })?;
        self.planroom.append_revision(
            RevisionKind::Reconcile,
            Some(desired.content_id),
            &outcomes,
        )
    }

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
            )?;
        }
        Ok(())
    }

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

    pub fn recover(&self) -> Result<()> {
        let _w = self.write.lock().unwrap();
        self.recover_locked()
    }

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
                    )?;
                }
            }
        }
        Ok(())
    }

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

fn empty_scroll(host: &str) -> Scroll {
    Scroll { name: host.to_string(), glyphs: vec![] }
}

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

fn has_terminal(steps: &[WalStep], intended: &WalStep) -> bool {
    steps.iter().any(|s| {
        s.seq > intended.seq
            && s.step_ord == intended.step_ord
            && s.action == intended.action
            && matches!(s.state, WalStepState::Done | WalStepState::Failed | WalStepState::Reversed)
    })
}

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

fn changed_file_path(op: &GlyphOp) -> Option<String> {
    match op.glyph() {
        Glyph::Filesystem { path, entry: Entry::File { .. } } => Some(path.clone()),
        _ => None,
    }
}

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
