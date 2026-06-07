//! The foreman: holds the Blueprints, resolves desired state, directs the
//! builder to realize each change (retrying transient failures, giving up
//! loudly on permanent ones), and journals every change as a Revision.

use anyhow::{bail, Result};
use golem_types::{Action, Blueprint, Host, Ingress, Revision, RevisionKind, Service, State, Workload};
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;
use tracing::warn;

use crate::builder::{BuildError, BuildResult, Builder};
use crate::planroom::PlanRoom;

pub struct Foreman {
    host: String,
    planroom: Box<dyn PlanRoom>,
    builder: Box<dyn Builder>,
    max_attempts: u32,
    retry_delay: Duration,
    /// Serializes the read-modify-write in commission/decommission so
    /// concurrent callers can't compute diffs against stale state or journal a
    /// State that never existed.
    write: Mutex<()>,
}

/// A pending change to the set of commissioned Blueprints.
enum Change {
    Commission(Blueprint),
    Decommission(String),
}

impl Foreman {
    pub fn new(host: String, planroom: Box<dyn PlanRoom>, builder: Box<dyn Builder>) -> Self {
        Self {
            host,
            planroom,
            builder,
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

    /// Commission a Blueprint: realize the change, then persist it. Nothing is
    /// stored or journalled unless the build succeeds.
    pub fn commission(&self, bp: Blueprint) -> Result<Revision> {
        Ok(self.apply(Change::Commission(bp))?.expect("commission always yields a revision"))
    }

    /// Decommission a Blueprint by name. `Ok(None)` if no such Blueprint.
    pub fn decommission(&self, name: &str) -> Result<Option<Revision>> {
        self.apply(Change::Decommission(name.to_string()))
    }

    /// The shared spine of commission/decommission: diff against current state,
    /// realize the change on this host, then persist the Blueprint and journal
    /// the Revision — all under the write lock, all-or-nothing. `Ok(None)` only
    /// when decommissioning a Blueprint that isn't present.
    fn apply(&self, change: Change) -> Result<Option<Revision>> {
        let _w = self.write.lock().unwrap();
        let mut active = self.planroom.blueprints()?;
        let prior = State::resolve(&active);

        let (kind, name) = match &change {
            Change::Commission(bp) => {
                active.insert(bp.name.clone(), bp.clone());
                (RevisionKind::Commission, bp.name.clone())
            }
            Change::Decommission(name) => {
                if active.remove(name).is_none() {
                    return Ok(None);
                }
                (RevisionKind::Decommission, name.clone())
            }
        };

        let next = State::resolve(&active);
        let actions = next.actions_from(&prior);
        self.realize(&actions, &active)?;

        match &change {
            Change::Commission(bp) => self.planroom.put_blueprint(bp)?,
            Change::Decommission(name) => self.planroom.delete_blueprint(name)?,
        }
        let rev = self.planroom.append_revision(kind, Some(name), &actions, &next)?;
        Ok(Some(rev))
    }

    pub fn state(&self) -> Result<State> {
        Ok(State::resolve(&self.planroom.blueprints()?))
    }

    pub fn blueprints(&self) -> Result<Vec<Blueprint>> {
        Ok(self.planroom.blueprints()?.into_values().collect())
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

    /// Direct the builder for every Action on *this* golem's host. Actions for
    /// other hosts are journalled but left to those golems.
    fn realize(&self, actions: &[Action], active: &BTreeMap<String, Blueprint>) -> Result<()> {
        for action in actions.iter().filter(|a| a.host() == self.host) {
            match action {
                Action::BuildWorkload { host, name } => {
                    let w = find(active, host, name, |h| &h.workloads)
                        .ok_or_else(|| missing("workload", host, name))?;
                    self.attempt(action, || self.builder.build_workload(host, w))?;
                }
                Action::TeardownWorkload { host, name } => {
                    self.attempt(action, || self.builder.teardown_workload(host, name))?;
                }
                Action::BuildService { host, name } => {
                    let s = find(active, host, name, |h| &h.services)
                        .ok_or_else(|| missing("service", host, name))?;
                    self.attempt(action, || self.builder.build_service(host, s))?;
                }
                Action::TeardownService { host, name } => {
                    self.attempt(action, || self.builder.teardown_service(host, name))?;
                }
                Action::BuildIngress { host, name } => {
                    let i = find(active, host, name, |h| &h.ingress)
                        .ok_or_else(|| missing("ingress", host, name))?;
                    self.attempt(action, || self.builder.build_ingress(host, i))?;
                }
                Action::TeardownIngress { host, name } => {
                    self.attempt(action, || self.builder.teardown_ingress(host, name))?;
                }
            }
        }
        Ok(())
    }

    /// Run one builder call, retrying retryable failures up to `max_attempts`.
    fn attempt(&self, action: &Action, mut run: impl FnMut() -> BuildResult) -> Result<()> {
        for n in 1..=self.max_attempts {
            match run() {
                Ok(()) => return Ok(()),
                Err(BuildError::Fatal(msg)) => bail!("{action:?}: fatal: {msg}"),
                Err(BuildError::Retryable(msg)) if n == self.max_attempts => {
                    bail!("{action:?}: gave up after {n} attempts: {msg}")
                }
                Err(BuildError::Retryable(msg)) => {
                    warn!(?action, attempt = n, "retryable failure: {msg}");
                    std::thread::sleep(self.retry_delay);
                }
            }
        }
        unreachable!("loop returns or bails")
    }
}

/// Find an item by (host, name) across all active Blueprints. `pick` selects
/// the item list (workloads/services/ingress) from a Host.
fn find<'a, T: Named>(
    active: &'a BTreeMap<String, Blueprint>,
    host: &str,
    name: &str,
    pick: impl Fn(&'a Host) -> &'a [T],
) -> Option<&'a T> {
    active
        .values()
        .flat_map(|bp| &bp.hosts)
        .filter(|h| h.name == host)
        .flat_map(|h| pick(h))
        .find(|item| item.name() == name)
}

trait Named {
    fn name(&self) -> &str;
}
impl Named for Workload {
    fn name(&self) -> &str {
        &self.name
    }
}
impl Named for Service {
    fn name(&self) -> &str {
        &self.name
    }
}
impl Named for Ingress {
    fn name(&self) -> &str {
        &self.name
    }
}

fn missing(kind: &str, host: &str, name: &str) -> anyhow::Error {
    anyhow::anyhow!("no {kind} {name:?} on host {host:?} to build")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planroom::MemoryPlanRoom;
    use std::sync::{Arc, Mutex};

    /// Records every call; always succeeds. Hand-written because it reads names.
    #[derive(Default)]
    struct Recorder {
        calls: Mutex<Vec<String>>,
    }
    impl Recorder {
        fn note(&self, s: String) -> BuildResult {
            self.calls.lock().unwrap().push(s);
            Ok(())
        }
        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
    }
    impl Builder for Recorder {
        fn build_workload(&self, h: &str, w: &Workload) -> BuildResult {
            self.note(format!("build workload {h}/{}", w.name))
        }
        fn teardown_workload(&self, h: &str, n: &str) -> BuildResult {
            self.note(format!("teardown workload {h}/{n}"))
        }
        fn build_service(&self, h: &str, s: &Service) -> BuildResult {
            self.note(format!("build service {h}/{}", s.name))
        }
        fn teardown_service(&self, h: &str, n: &str) -> BuildResult {
            self.note(format!("teardown service {h}/{n}"))
        }
        fn build_ingress(&self, h: &str, i: &Ingress) -> BuildResult {
            self.note(format!("build ingress {h}/{}", i.name))
        }
        fn teardown_ingress(&self, h: &str, n: &str) -> BuildResult {
            self.note(format!("teardown ingress {h}/{n}"))
        }
    }

    /// For mock builders that ignore their typed args and run one body per call.
    macro_rules! uniform_builder {
        ($t:ty, |$s:ident| $body:expr) => {
            impl Builder for $t {
                fn build_workload(&$s, _: &str, _: &Workload) -> BuildResult { $body }
                fn teardown_workload(&$s, _: &str, _: &str) -> BuildResult { $body }
                fn build_service(&$s, _: &str, _: &Service) -> BuildResult { $body }
                fn teardown_service(&$s, _: &str, _: &str) -> BuildResult { $body }
                fn build_ingress(&$s, _: &str, _: &Ingress) -> BuildResult { $body }
                fn teardown_ingress(&$s, _: &str, _: &str) -> BuildResult { $body }
            }
        };
    }

    /// Fails retryably `fails` times, then succeeds. Counts calls.
    struct FlakyThenOk {
        fails_left: Mutex<u32>,
        calls: Mutex<u32>,
    }
    impl FlakyThenOk {
        fn new(fails: u32) -> Self {
            Self { fails_left: Mutex::new(fails), calls: Mutex::new(0) }
        }
        fn outcome(&self) -> BuildResult {
            *self.calls.lock().unwrap() += 1;
            let mut left = self.fails_left.lock().unwrap();
            if *left > 0 {
                *left -= 1;
                Err(BuildError::Retryable("flaky".into()))
            } else {
                Ok(())
            }
        }
    }
    uniform_builder!(FlakyThenOk, |self| self.outcome());

    /// Always fails with the given error class. Counts calls.
    struct Failing {
        make: fn(String) -> BuildError,
        calls: Mutex<u32>,
    }
    impl Failing {
        fn new(make: fn(String) -> BuildError) -> Self {
            Self { make, calls: Mutex::new(0) }
        }
        fn calls(&self) -> u32 {
            *self.calls.lock().unwrap()
        }
    }
    uniform_builder!(Failing, |self| {
        *self.calls.lock().unwrap() += 1;
        Err((self.make)("nope".into()))
    });

    fn foreman(host: &str, builder: Box<dyn Builder>) -> Foreman {
        Foreman::new(host.into(), Box::new(MemoryPlanRoom::new()), builder).with_retry(3, Duration::ZERO)
    }

    fn host(name: &str, services: &[&str]) -> Host {
        Host {
            name: name.into(),
            services: services.iter().map(|s| Service { name: (*s).into(), ..Default::default() }).collect(),
            ..Default::default()
        }
    }

    fn bp(name: &str, hosts: Vec<Host>) -> Blueprint {
        Blueprint { name: name.into(), hosts }
    }

    #[test]
    fn commission_builds_local_host_only_but_journals_every_host() {
        let rec = Arc::new(Recorder::default());
        let f = foreman("h1", Box::new(rec.clone()));
        let rev = f.commission(bp("web", vec![host("h1", &["app"]), host("h2", &["other"])])).unwrap();

        assert_eq!(rev.kind, RevisionKind::Commission);
        assert_eq!(rec.calls(), vec!["build service h1/app"]); // only the local host is built
        // The other host's action is still recorded — it's another golem's to build.
        assert!(rev.actions.contains(&Action::BuildService { host: "h2".into(), name: "other".into() }));
        assert_eq!(f.revisions().unwrap().len(), 2); // Init + this
    }

    #[test]
    fn decommission_tears_down_unique_keeps_shared() {
        let rec = Arc::new(Recorder::default());
        let f = foreman("h1", Box::new(rec.clone()));
        f.commission(bp("a", vec![host("h1", &["nginx"])])).unwrap();
        f.commission(bp("b", vec![host("h1", &["nginx", "pg"])])).unwrap();
        rec.calls.lock().unwrap().clear();

        f.decommission("b").unwrap();

        assert_eq!(rec.calls(), vec!["teardown service h1/pg"]); // nginx stays (a wants it)
        assert!(f.state().unwrap().hosts["h1"].services.contains_key("nginx"));
    }

    #[test]
    fn decommission_unknown_is_none() {
        let f = foreman("h1", Box::new(Recorder::default()));
        assert!(f.decommission("ghost").unwrap().is_none());
    }

    #[test]
    fn retryable_failures_are_retried_until_success() {
        let flaky = Arc::new(FlakyThenOk::new(2));
        let f = foreman("h1", Box::new(flaky.clone()));
        f.commission(bp("a", vec![host("h1", &["app"])])).unwrap();
        assert_eq!(*flaky.calls.lock().unwrap(), 3); // 2 failures + 1 success
    }

    #[test]
    fn no_retry_config_attempts_once() {
        let failing = Arc::new(Failing::new(BuildError::Retryable));
        let f = Foreman::new("h1".into(), Box::new(MemoryPlanRoom::new()), Box::new(failing.clone()))
            .with_retry(1, Duration::ZERO);
        assert!(f.commission(bp("a", vec![host("h1", &["app"])])).is_err());
        assert_eq!(failing.calls(), 1);
    }

    #[test]
    fn exhausted_retries_fail_loudly_and_persist_nothing() {
        let failing = Arc::new(Failing::new(BuildError::Retryable));
        let f = foreman("h1", Box::new(failing.clone()));
        let err = f.commission(bp("a", vec![host("h1", &["app"])])).unwrap_err();

        assert!(err.to_string().contains("gave up"));
        assert_eq!(failing.calls(), 3); // tried max_attempts times
        assert!(f.blueprints().unwrap().is_empty());
        assert_eq!(f.revisions().unwrap().len(), 1); // only Init
    }

    #[test]
    fn fatal_failure_is_not_retried_and_persists_nothing() {
        let failing = Arc::new(Failing::new(BuildError::Fatal));
        let f = foreman("h1", Box::new(failing.clone()));
        let err = f.commission(bp("a", vec![host("h1", &["app"])])).unwrap_err();

        assert!(err.to_string().contains("fatal"));
        assert_eq!(failing.calls(), 1); // fatal short-circuits the retry loop
        assert!(f.blueprints().unwrap().is_empty());
        assert_eq!(f.revisions().unwrap().len(), 1);
    }
}
