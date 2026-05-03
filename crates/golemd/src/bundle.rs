//! Bundle ingestion.
//!
//! Pipeline:
//!   1. Parse SignedBundle JSON.
//!   2. Verify ed25519 signature over canonical JSON of the inner Bundle.
//!   3. Verify signer is in the trusted-keys set.
//!   4. Verify bundle.node matches our node name.
//!   5. Verify version is monotonic (>= last accepted).
//!   6. Expand Quadlet claims into File + SystemdUnit + Handler.
//!   7. Merge claims with the same ClaimId — union owners, assert spec equality.
//!
//! After this, the reconciler sees a flat, primitives-only claim set.

use anyhow::{anyhow, bail, Context, Result};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use golem_types::{canonical_json, Bundle, Claim, ClaimId, ClaimSpec, OwnerId, SignedBundle};
use std::collections::{BTreeSet, HashMap, HashSet};

use crate::providers::quadlet::expand_quadlets;

#[derive(Clone, Debug)]
pub struct TrustConfig {
    pub node_name:    String,
    pub trusted_keys: HashSet<String>,   // hex-encoded ed25519 public keys
}

pub fn load_signed(json: &[u8], trust: &TrustConfig, last_version: Option<u64>) -> Result<Bundle> {
    let sb: SignedBundle = serde_json::from_slice(json).context("parse SignedBundle")?;

    // 1. Trusted signer?
    if !trust.trusted_keys.contains(&sb.signer_pk) {
        bail!("untrusted signer: {}", sb.signer_pk);
    }

    // 2. Signature check over canonical JSON of the inner bundle.
    let canonical = canonical_json(&sb.bundle).context("canonicalize bundle for verify")?;
    let pk_bytes: [u8; 32] = hex::decode(&sb.signer_pk)
        .context("decode pk hex")?
        .try_into()
        .map_err(|_| anyhow!("signer_pk is not 32 bytes"))?;
    let vk = VerifyingKey::from_bytes(&pk_bytes).context("parse pk")?;
    let sig_bytes: [u8; 64] = hex::decode(&sb.signature)
        .context("decode sig hex")?
        .try_into()
        .map_err(|_| anyhow!("signature is not 64 bytes"))?;
    let sig = Signature::from_bytes(&sig_bytes);
    vk.verify(&canonical, &sig).context("signature verification")?;

    // 3. Node match.
    if sb.bundle.node != trust.node_name {
        bail!(
            "bundle is for node `{}` but I am `{}`",
            sb.bundle.node, trust.node_name
        );
    }

    // 4. Strictly monotonic version: reject version <= last accepted.
    // Equality would let an attacker replay the most-recent signed bundle
    // after-the-fact (e.g., to undo a subsequent version bump if the operator
    // briefly downgraded the bundle). Forcing the operator to bump version
    // even for cosmetic re-pushes is cheap.
    if let Some(prev) = last_version {
        if sb.bundle.version <= prev {
            bail!(
                "bundle version {} is not strictly greater than last accepted {}",
                sb.bundle.version, prev
            );
        }
    }

    // 5+6: expand quadlets, then merge duplicate IDs.
    let expanded = expand_quadlets(sb.bundle);
    let merged   = merge_claims(expanded)?;
    Ok(merged)
}

/// Two claims with the same ClaimId must have spec-equal specs; their owners
/// are unioned. This is how a single resource (e.g. apt:podman) can be
/// claimed by multiple workloads without conflict.
fn merge_claims(mut bundle: Bundle) -> Result<Bundle> {
    let mut by_id: HashMap<ClaimId, Claim> = HashMap::new();

    for c in bundle.claims.drain(..) {
        match by_id.get_mut(&c.id) {
            None => {
                by_id.insert(c.id.clone(), c);
            }
            Some(existing) => {
                if !specs_equivalent(&existing.spec, &c.spec) {
                    bail!(
                        "claim {} has conflicting specs across owners {:?} vs {:?}",
                        c.id, existing.owners, c.owners
                    );
                }
                let new_owners: BTreeSet<OwnerId> =
                    existing.owners.union(&c.owners).cloned().collect();
                existing.owners = new_owners;

                // Union `after` deps, deduped (cheap; lists are tiny).
                for dep in c.after {
                    if !existing.after.contains(&dep) {
                        existing.after.push(dep);
                    }
                }
            }
        }
    }

    bundle.claims = by_id.into_values().collect();
    Ok(bundle)
}

/// Spec equivalence — JSON-level. Good enough; the alternative is per-variant
/// PartialEq impls and we don't need that level of nuance for M1.
fn specs_equivalent(a: &ClaimSpec, b: &ClaimSpec) -> bool {
    match (serde_json::to_value(a), serde_json::to_value(b)) {
        (Ok(va), Ok(vb)) => va == vb,
        _ => false,
    }
}
