//! Reconciler.
//!
//! Single tick (see DESIGN.md §6 for the crash-invariant analysis):
//!
//!   1. Snapshot desired (in-memory authoritative claim set).
//!   2. Load all recorded ClaimStates from the journal.
//!   3. Phase 1 — capture-once: for each desired claim with `captured=false`
//!      in the journal, run provider.capture (read-only) and persist. This
//!      runs BEFORE any mutation so that a later claim's "preexisting"
//!      reading is honest with respect to a previous claim's mutate.
//!   4. Orphan sweep: for each recorded id no longer in desired, unmutate
//!      using the durable capture, then forget.
//!   5. Phase 2 — mutate: topo-order desired by `after` edges. For each
//!      claim, if matches() already, skip; else journal intent (applied=false),
//!      call mutate(spec, &capture), journal applied=true.
//!   6. Daemon-reload (debounced) + handlers.
//!
//! Failure of one resource never blocks the rest. The next tick is the
//! retry mechanism — there are no in-tick retries, because the journal-
//! before-mutate discipline already guarantees honest recovery.

use anyhow::Result;
use chrono::Utc;
use golem_types::{
    Bundle, CaptureError, Claim, ClaimId, ClaimSpec, ClaimState, Health, ProviderKind, Scope,
};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::deps::topo_order;
use crate::providers::{maybe_crash, provider_for, systemd_unit, Observation};
use crate::state::Store;

#[derive(Default, Debug)]
pub struct TickReport {
    pub captured:       Vec<ClaimId>,
    pub applied:        Vec<ClaimId>,
    pub already_ok:     Vec<ClaimId>,
    pub orphaned:       Vec<ClaimId>,
    pub degraded:       Vec<(ClaimId, String)>,
    pub errors:         Vec<(ClaimId, String)>,
    pub refused:        Vec<(ClaimId, String)>,
    pub handlers_fired: Vec<String>,
}

pub struct Reconciler {
    pub store:   Arc<Store>,
    pub bundle:  Arc<RwLock<Option<Bundle>>>,
}

impl Reconciler {
    pub fn new(store: Arc<Store>, bundle: Arc<RwLock<Option<Bundle>>>) -> Self {
        Self { store, bundle }
    }

    pub async fn tick(&self) -> Result<TickReport> {
        let mut report = TickReport::default();

        let bundle = match self.bundle.read().await.clone() {
            Some(b) => b,
            None => {
                debug!("no bundle yet, skipping tick");
                return Ok(report);
            }
        };

        let desired: HashMap<ClaimId, Claim> =
            bundle.claims.iter().map(|c| (c.id.clone(), c.clone())).collect();

        let mut recorded: HashMap<ClaimId, ClaimState> = self
            .store
            .load_all()?
            .into_iter()
            .map(|s| (s.id.clone(), s))
            .collect();

        // ── Phase 1: capture-once for every unfamiliar claim ──────────────
        // Runs BEFORE any mutation so apt's postinst can't pollute a later
        // File claim's notion of "preexisting." See DESIGN.md §6.
        for claim in &bundle.claims {
            let already_captured = recorded
                .get(&claim.id)
                .map(|s| s.captured)
                .unwrap_or(false);
            if already_captured { continue; }

            let provider = provider_for(&claim.spec);
            match provider.capture(&claim.spec).await {
                Ok(capture) => {
                    let mut st = recorded
                        .get(&claim.id)
                        .cloned()
                        .unwrap_or_else(|| ClaimState::fresh(claim.id.clone()));
                    st.captured     = true;
                    st.preexisting  = capture.preexisting;
                    st.backup       = capture.backup;
                    st.last_spec    = Some(claim.spec.clone());
                    self.store.put(&st)?;
                    recorded.insert(claim.id.clone(), st);
                    report.captured.push(claim.id.clone());
                    maybe_crash("capture_persisted");
                }
                Err(CaptureError::TooLarge(n)) => {
                    let msg = format!("capture too large: {n} bytes (cap {})",
                        golem_types::MAX_CAPTURE_BYTES);
                    warn!("refusing claim {}: {msg}", claim.id);
                    report.refused.push((claim.id.clone(), msg));
                }
                Err(CaptureError::Other(e)) => {
                    error!("capture {} failed: {e:#}", claim.id);
                    report.errors.push((claim.id.clone(), format!("capture: {e:#}")));
                }
            }
        }

        // ── Orphan sweep ──────────────────────────────────────────────────
        // Reverse order: stop services before deleting files before purging
        // packages. We don't have desired-state ordering for orphans (they're
        // not in `desired`), so we use ProviderKind ordering as a heuristic.
        let mut orphan_ids: Vec<ClaimId> = recorded
            .keys()
            .filter(|id| !desired.contains_key(id))
            .cloned()
            .collect();
        orphan_ids.sort_by_key(|id| match id.kind {
            ProviderKind::SystemdUnit => 0,
            ProviderKind::Quadlet     => 1,
            ProviderKind::CaddySite   => 2,
            ProviderKind::NftFragment => 3,
            ProviderKind::File        => 4,
            ProviderKind::AptPackage  => 5,
        });

        for id in orphan_ids {
            let st = recorded.get(&id).cloned().unwrap();
            let spec = match st.last_spec.clone() {
                Some(s) => s,
                None => {
                    warn!("orphan {} has no recorded spec, skipping unmutate", id);
                    self.store.forget(&id)?;
                    continue;
                }
            };
            let capture = st.capture();
            let provider = provider_for(&spec);
            match provider.unmutate(&spec, &capture).await {
                Ok(()) => {
                    self.store.forget(&id)?;
                    recorded.remove(&id);
                    report.orphaned.push(id);
                }
                Err(e) => {
                    error!("unmutate {} failed: {e:#}", id);
                    report.errors.push((id, format!("unmutate: {e:#}")));
                }
            }
        }

        // ── Phase 2: mutate ───────────────────────────────────────────────
        let order = match topo_order(&bundle.claims) {
            Ok(o) => o,
            Err(e) => {
                error!("topo sort failed: {e:#}");
                return Ok(report);
            }
        };

        let mut changed_files: HashSet<ClaimId> = HashSet::new();

        for id in order {
            let claim = match desired.get(&id) {
                Some(c) => c,
                None => continue,
            };

            // A claim refused at capture-time isn't in `recorded` with
            // captured=true, so we skip it here.
            let st_present = recorded.get(&id).map(|s| s.captured).unwrap_or(false);
            if !st_present {
                debug!("skipping mutate for {} (no captured state)", id);
                continue;
            }

            let provider = provider_for(&claim.spec);

            // Cheap fast path: already matches.
            match provider.observe(&claim.spec).await {
                Ok(Observation::Present) => {
                    if provider.matches(&claim.spec).await.unwrap_or(false) {
                        report.already_ok.push(id.clone());
                        continue;
                    }
                }
                Ok(Observation::Absent) => { /* fall through to mutate */ }
                Err(e) => {
                    warn!("observe {} failed: {e:#}", id);
                }
            }

            // Journal intent BEFORE mutate. The capture is already durable
            // from phase 1; this entry just records that we're about to act.
            let mut st = recorded.get(&id).cloned().unwrap();
            st.last_spec = Some(claim.spec.clone());
            self.store.put(&st)?;
            maybe_crash("intent_applied_false");

            let prev_hash = st.content_hash.clone();
            let capture = st.capture();

            match provider.mutate(&claim.spec, &capture).await {
                Ok(()) => {
                    maybe_crash("mutate_completed");

                    st.last_applied = Some(Utc::now());

                    if let ClaimSpec::File(f) = &claim.spec {
                        let new_hash = sha256_hex(f.content.as_bytes());
                        if Some(&new_hash) != prev_hash.as_ref() {
                            changed_files.insert(id.clone());
                        }
                        st.content_hash = Some(new_hash);
                    }

                    st.last_health = Some(
                        provider.check(&claim.spec).await.unwrap_or(Health::Unknown),
                    );
                    self.store.put(&st)?;
                    maybe_crash("intent_applied_true");

                    recorded.insert(id.clone(), st);

                    match recorded.get(&id).and_then(|s| s.last_health.as_ref()) {
                        Some(Health::Degraded(msg)) => {
                            report.degraded.push((id, msg.clone()))
                        }
                        _ => report.applied.push(id),
                    }
                }
                Err(e) => {
                    error!("mutate {} failed: {e:#}", id);
                    report.errors.push((id, format!("mutate: {e:#}")));
                    self.store.put(&st)?;
                }
            }
        }

        // ── Daemon-reload (debounced) ─────────────────────────────────────
        // Only reload if a file under a systemd-watched path actually changed
        // this tick. Avoids the eager-reload heuristic flagged in REVIEW.md.
        let needs_reload = changed_files.iter().any(|id| {
            let p = &id.key;
            p.starts_with("/etc/containers/systemd/")
                || p.starts_with("/etc/systemd/system/")
                || p.starts_with("/run/systemd/system/")
        });
        if needs_reload {
            if let Err(e) = systemd_unit::daemon_reload(Scope::System).await {
                warn!("daemon-reload failed: {e:#}");
            } else {
                debug!("daemon-reload fired");
            }
        }

        // ── Handlers ──────────────────────────────────────────────────────
        for h in &bundle.handlers {
            if changed_files.contains(&h.source) {
                for unit in &h.targets {
                    match systemd_unit::restart_unit(Scope::System, unit).await {
                        Ok(()) => report.handlers_fired.push(unit.clone()),
                        Err(e) => {
                            warn!("handler restart {unit} failed: {e:#}");
                            report.errors.push((
                                h.source.clone(),
                                format!("handler restart {unit}: {e:#}"),
                            ));
                        }
                    }
                }
            }
        }

        info!(
            "tick: captured={} applied={} ok={} orphaned={} degraded={} errors={} refused={} handlers={}",
            report.captured.len(),
            report.applied.len(),
            report.already_ok.len(),
            report.orphaned.len(),
            report.degraded.len(),
            report.errors.len(),
            report.refused.len(),
            report.handlers_fired.len(),
        );
        Ok(report)
    }

    pub async fn run_forever(self, period: Duration) {
        loop {
            if let Err(e) = self.tick().await {
                error!("tick error: {e:#}");
            }
            let jitter = fastrand::u64(0..(period.as_millis() as u64 / 4).max(1));
            tokio::time::sleep(period + Duration::from_millis(jitter)).await;
        }
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}
