//! Quadlet provider.
//!
//! A Quadlet isn't a real OS resource; it's a pair (file + unit) with a
//! handler edge (when the .container file changes, run `systemctl
//! daemon-reload` and restart the generated service).
//!
//! We handle this two ways, either of which works:
//!
//!   (A) The bundle can emit the File + SystemdUnit claims directly,
//!       with `Quadlet` never appearing. The Nickel schema already does
//!       this for us (see workload.ncl — `workload_claims_for`).
//!
//!   (B) The bundle can emit a single `Quadlet` claim, and this provider
//!       expands it at reconcile time to the pair.
//!
//! We keep (B) available as an ergonomic fallback so simple hand-written
//! bundles can say `{"kind":"quadlet", ...}`. The agent's bundle loader
//! flattens these into the underlying File + SystemdUnit claims *before*
//! they hit the reconciler — so by the time anything else runs, it's all
//! primitives. This provider's methods therefore should never be called
//! in practice; they're here for completeness.

use anyhow::{bail, Result};
use async_trait::async_trait;
use golem_types::{Capture, CaptureError, ClaimSpec, Health};

use super::{Observation, Provider};

pub struct QuadletProvider;

#[async_trait]
impl Provider for QuadletProvider {
    async fn observe(&self, _spec: &ClaimSpec) -> Result<Observation> {
        bail!("Quadlet claims must be expanded to File + SystemdUnit before reconcile");
    }
    async fn matches(&self, _spec: &ClaimSpec) -> Result<bool> { Ok(false) }
    async fn capture(&self, _spec: &ClaimSpec) -> Result<Capture, CaptureError> {
        Err(CaptureError::Other(anyhow::anyhow!(
            "Quadlet claims must be expanded before reconcile"
        )))
    }
    async fn mutate(&self, _spec: &ClaimSpec, _capture: &Capture) -> Result<()> {
        bail!("Quadlet claims must be expanded before reconcile")
    }
    async fn unmutate(&self, _spec: &ClaimSpec, _capture: &Capture) -> Result<()> { Ok(()) }
    async fn check(&self, _spec: &ClaimSpec) -> Result<Health> { Ok(Health::Unknown) }
}

// ─── The expander ──────────────────────────────────────────────────────────
//
// Called by bundle.rs on load. Replaces each Quadlet claim with a File claim
// (the .container definition) and a SystemdUnit claim (the generated service),
// inheriting owners and injecting the right `after` edges. Also emits a
// Handler so the reconciler will daemon-reload + restart on content change.

use golem_types::{
    Bundle, Claim, ClaimId, FileMarker, FileSpec, Handler, ProviderKind, Scope,
    SystemdUnitSpec,
};

pub fn expand_quadlets(mut bundle: Bundle) -> Bundle {
    let mut expanded: Vec<Claim> = Vec::with_capacity(bundle.claims.len() * 2);
    let mut handlers = std::mem::take(&mut bundle.handlers);

    for claim in bundle.claims.drain(..) {
        match &claim.spec {
            ClaimSpec::Quadlet { name, body, active } => {
                let file_path = format!("/etc/containers/systemd/{name}.container");
                let unit_name = format!("{name}.service");
                let file_id = ClaimId { kind: ProviderKind::File, key: file_path.clone() };
                let unit_id = ClaimId { kind: ProviderKind::SystemdUnit, key: unit_name.clone() };

                expanded.push(Claim {
                    id: file_id.clone(),
                    spec: ClaimSpec::File(FileSpec {
                        path:    file_path.clone(),
                        content: body.clone(),
                        mode:    0o644,
                        owner:   "root".into(),
                        group:   "root".into(),
                        marker:  FileMarker::Owned,
                    }),
                    owners: claim.owners.clone(),
                    after:  claim.after.clone(),
                });
                expanded.push(Claim {
                    id: unit_id,
                    spec: ClaimSpec::SystemdUnit(SystemdUnitSpec {
                        name:   unit_name.clone(),
                        enable: true,
                        active: *active,
                        scope:  Scope::System,
                    }),
                    owners: claim.owners.clone(),
                    after:  vec![file_id.clone()],
                });

                // Handler: when the .container file changes, daemon-reload +
                // restart the service. daemon-reload is implicit — the
                // reconciler does it once per tick if any quadlet file changed.
                handlers.push(Handler {
                    source:  file_id,
                    targets: vec![unit_name],
                });
            }
            _ => expanded.push(claim),
        }
    }

    bundle.claims = expanded;
    bundle.handlers = handlers;
    bundle
}
