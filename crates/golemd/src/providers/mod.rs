//! The Provider abstraction.
//!
//! Each resource kind has a Provider that knows how to observe, capture
//! prior state, mutate the OS toward a spec, and reverse a mutation. The
//! reconciler (see `reconcile.rs`) drives them in a phased loop:
//!
//!   Phase 1 — for every claim that has not yet been captured, call
//!             `capture` and journal the result. This is read-only against
//!             the OS and runs before any mutation in the tick.
//!
//!   Phase 2 — for every claim that doesn't already match the spec, journal
//!             intent=Apply with applied=false, call `mutate` (with the
//!             durable Capture as a hint), then journal applied=true.
//!
//! Why phased: if claim A's mutate creates the resource claim B is about
//! to capture (e.g., apt installs a package whose postinst writes a file
//! that's then a File claim), capture-just-before-mutate would record the
//! apt-default as `preexisting=true` — falsely. Phase 1 captures every
//! claim's prior state before any phase-2 mutation runs. See DESIGN.md §6.
//!
//! `mutate` MUST NOT touch engine state (ClaimState fields). The reconciler
//! owns the journal; providers own the OS.

use anyhow::Result;
use async_trait::async_trait;
use golem_types::{Capture, CaptureError, ClaimSpec, Health};

pub mod apt;
pub mod file;
pub mod quadlet;
pub mod systemd_unit;

/// What `observe` returns. Caller decides whether it matches desired.
pub enum Observation {
    Absent,
    Present,
}

#[async_trait]
pub trait Provider: Send + Sync {
    /// Cheap probe. Must be fast — runs every reconcile tick for every claim.
    async fn observe(&self, spec: &ClaimSpec) -> Result<Observation>;

    /// Does current reality match desired spec? Called after observe == Present.
    /// Separate from observe so callers can cache the expensive parts.
    async fn matches(&self, spec: &ClaimSpec) -> Result<bool>;

    /// Read-only. Capture everything needed to honor unapply later.
    /// Runs once per claim id, in phase 1 of the tick, before any mutation.
    /// Must be safe to call repeatedly (idempotent), but the reconciler
    /// guarantees it only persists the first call's output.
    ///
    /// Returns `CaptureError::TooLarge` if prior state exceeds
    /// `MAX_CAPTURE_BYTES`. The engine surfaces this as a refused claim
    /// rather than silently OOM-ing.
    async fn capture(&self, spec: &ClaimSpec) -> Result<Capture, CaptureError>;

    /// Mutate the OS toward `spec`. May internally re-observe the OS to
    /// converge after a crashed previous mutate. The `capture` hint provides
    /// the durable prior-state record (e.g., previous content_hash for
    /// change-detection, prior_active for restart suppression).
    ///
    /// MUST NOT write to engine state — the reconciler journals around this
    /// call. Returns `Ok(())` on success; the reconciler then journals
    /// applied=true.
    async fn mutate(&self, spec: &ClaimSpec, capture: &Capture) -> Result<()>;

    /// Reverse `mutate`, using the captured prior state. Must honor
    /// `capture.preexisting`: if true, restore prior state and do NOT remove
    /// the resource entirely.
    async fn unmutate(&self, spec: &ClaimSpec, capture: &Capture) -> Result<()>;

    /// Post-mutate health gate. Cheap; run after mutate or on a slower cadence.
    async fn check(&self, spec: &ClaimSpec) -> Result<Health>;
}

/// Dispatch a ClaimSpec to its provider. Boxing is fine; we do it once per
/// claim per tick and the cost is noise against dpkg-query forks.
pub fn provider_for(spec: &ClaimSpec) -> Box<dyn Provider> {
    match spec {
        ClaimSpec::File(_)        => Box::new(file::FileProvider),
        ClaimSpec::AptPackage(_)  => Box::new(apt::AptProvider::global()),
        ClaimSpec::SystemdUnit(_) => Box::new(systemd_unit::SystemdUnitProvider),
        ClaimSpec::Quadlet { .. } => Box::new(quadlet::QuadletProvider),
    }
}

// ─── Crash-injection points (test-only) ────────────────────────────────────
//
// The smoke test exercises journal-before-mutate by setting GOLEM_CRASH_AFTER
// to one of these labels and verifying the next tick converges honestly.
//
// We std::process::abort() rather than panic!() to defeat any panic hooks,
// catch_unwind, or drop guards that would mask a real SIGKILL. Under abort,
// no tokio runtime cleanup runs; only the SQLite WAL fsync and OS write
// barriers from prior journal puts are durable.

pub fn maybe_crash(label: &str) {
    if let Ok(val) = std::env::var("GOLEM_CRASH_AFTER") {
        if val == label {
            tracing::error!("GOLEM_CRASH_AFTER={label}: aborting for crash-recovery test");
            std::process::abort();
        }
    }
}
