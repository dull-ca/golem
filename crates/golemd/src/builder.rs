//! The builder: the hands that realize a resolved Blueprint on a concrete
//! platform. [`Builder`] is the trait; real platform builders (apt/nftables/
//! quadlet, docker-compose, …) implement it. [`RandomBuilder`] is the stand-in
//! until one exists: it waits a short random time, then succeeds or fails.

use golem_types::{Ingress, Service, Workload};
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

#[derive(Debug)]
pub enum BuildError {
    /// Transient: worth retrying.
    Retryable(String),
    /// Permanent: give up.
    Fatal(String),
}

pub type BuildResult = Result<(), BuildError>;

/// Realizes one item. Implementations must be **idempotent**: `build_*` ensures
/// the item is present, `teardown_*` ensures it is absent. The foreman retries
/// failed calls and may replay them after a restart, so any call can run more
/// than once with the same arguments.
pub trait Builder: Send + Sync {
    fn build_workload(&self, host: &str, workload: &Workload) -> BuildResult;
    fn teardown_workload(&self, host: &str, name: &str) -> BuildResult;
    fn build_service(&self, host: &str, service: &Service) -> BuildResult;
    fn teardown_service(&self, host: &str, name: &str) -> BuildResult;
    fn build_ingress(&self, host: &str, ingress: &Ingress) -> BuildResult;
    fn teardown_ingress(&self, host: &str, name: &str) -> BuildResult;
}

impl<B: Builder + ?Sized> Builder for Arc<B> {
    fn build_workload(&self, host: &str, w: &Workload) -> BuildResult {
        (**self).build_workload(host, w)
    }
    fn teardown_workload(&self, host: &str, name: &str) -> BuildResult {
        (**self).teardown_workload(host, name)
    }
    fn build_service(&self, host: &str, s: &Service) -> BuildResult {
        (**self).build_service(host, s)
    }
    fn teardown_service(&self, host: &str, name: &str) -> BuildResult {
        (**self).teardown_service(host, name)
    }
    fn build_ingress(&self, host: &str, i: &Ingress) -> BuildResult {
        (**self).build_ingress(host, i)
    }
    fn teardown_ingress(&self, host: &str, name: &str) -> BuildResult {
        (**self).teardown_ingress(host, name)
    }
}

/// Stand-in builder: a random delay, then success or a retryable/fatal failure.
pub struct RandomBuilder {
    pub max_delay: Duration,
    pub retryable_pct: u8,
    pub fatal_pct: u8,
}

impl Default for RandomBuilder {
    fn default() -> Self {
        Self { max_delay: Duration::from_millis(250), retryable_pct: 25, fatal_pct: 5 }
    }
}

impl RandomBuilder {
    fn simulate(&self, what: &str) -> BuildResult {
        std::thread::sleep(Duration::from_millis(entropy() % (self.max_delay.as_millis() as u64 + 1)));
        let r = (entropy() % 100) as u8;
        if r < self.fatal_pct {
            Err(BuildError::Fatal(format!("{what}: simulated permanent failure")))
        } else if r < self.fatal_pct + self.retryable_pct {
            Err(BuildError::Retryable(format!("{what}: simulated transient failure")))
        } else {
            Ok(())
        }
    }
}

impl Builder for RandomBuilder {
    fn build_workload(&self, host: &str, w: &Workload) -> BuildResult {
        info!(host, workload = %w.name, "build");
        self.simulate("workload")
    }
    fn teardown_workload(&self, host: &str, name: &str) -> BuildResult {
        info!(host, workload = name, "teardown");
        self.simulate("workload")
    }
    fn build_service(&self, host: &str, s: &Service) -> BuildResult {
        info!(host, service = %s.name, "build");
        self.simulate("service")
    }
    fn teardown_service(&self, host: &str, name: &str) -> BuildResult {
        info!(host, service = name, "teardown");
        self.simulate("service")
    }
    fn build_ingress(&self, host: &str, i: &Ingress) -> BuildResult {
        info!(host, ingress = %i.name, "build");
        self.simulate("ingress")
    }
    fn teardown_ingress(&self, host: &str, name: &str) -> BuildResult {
        info!(host, ingress = name, "teardown");
        self.simulate("ingress")
    }
}

/// Cheap, non-cryptographic source of variation for the stand-in builder.
fn entropy() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.subsec_nanos() as u64).unwrap_or(0)
}
