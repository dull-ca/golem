//! Shared types between `golemd` and `golemctl`.
//!
//! The Bundle is what crosses the trust boundary: signed by the operator,
//! verified by the agent. Everything else is internal to the agent.

pub mod canonical;
pub use canonical::canonical_json;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

// ─── Identity ──────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    File,
    AptPackage,
    SystemdUnit,
    // M2+ — still addressable as an id even though the provider is virtual:
    Quadlet,
    // Future:
    NftFragment,
    CaddySite,
}

impl ProviderKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderKind::File => "file",
            ProviderKind::AptPackage => "apt_package",
            ProviderKind::SystemdUnit => "systemd_unit",
            ProviderKind::Quadlet => "quadlet",
            ProviderKind::NftFragment => "nft_fragment",
            ProviderKind::CaddySite => "caddy_site",
        }
    }
}

#[derive(Clone, Debug, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct ClaimId {
    pub kind: ProviderKind,
    pub key:  String,
}

impl std::fmt::Display for ClaimId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.kind.as_str(), self.key)
    }
}

// ─── Specs ─────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum FileMarker {
    /// We own the whole file. On unapply: restore backup if preexisting, else delete.
    Owned,
    /// Snippet wrapped in BEGIN/END markers inside a file we don't own.
    BlockInFile,
    /// File under a `.d/` drop-in dir — treated like Owned for us.
    Dropin,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileSpec {
    pub path:    String,
    pub content: String,
    #[serde(default = "default_mode")]
    pub mode:    u32,
    #[serde(default = "default_root")]
    pub owner:   String,
    #[serde(default = "default_root")]
    pub group:   String,
    #[serde(default = "default_marker")]
    pub marker:  FileMarker,
}
fn default_mode()   -> u32    { 0o644 }
fn default_root()   -> String { "root".into() }
fn default_marker() -> FileMarker { FileMarker::Owned }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AptPackageSpec {
    pub name:    String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub hold:    bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Scope { System, User }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SystemdUnitSpec {
    pub name:    String,
    #[serde(default = "default_true")]
    pub enable:  bool,
    #[serde(default = "default_true")]
    pub active:  bool,
    #[serde(default = "default_scope")]
    pub scope:   Scope,
}
fn default_true()  -> bool  { true }
fn default_scope() -> Scope { Scope::System }

// ─── ClaimSpec (tagged union over the specs) ───────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", content = "spec", rename_all = "snake_case")]
pub enum ClaimSpec {
    File(FileSpec),
    AptPackage(AptPackageSpec),
    SystemdUnit(SystemdUnitSpec),
    // Quadlet is expanded client-side (by the agent, before reconcile)
    // into a File + SystemdUnit. But we accept it in the bundle for
    // ergonomics.
    Quadlet { name: String, body: String, active: bool },
}

// ─── The user-facing Claim ─────────────────────────────────────────────────

pub type OwnerId = String;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Claim {
    pub id:     ClaimId,
    #[serde(flatten)]
    pub spec:   ClaimSpec,
    pub owners: BTreeSet<OwnerId>,
    #[serde(default)]
    pub after:  Vec<ClaimId>,
}

// ─── Handlers (Ansible-style) ──────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Handler {
    pub source:  ClaimId,
    pub targets: Vec<String>,   // unit names to restart
}

// ─── Bundle (over the wire) ────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Bundle {
    pub version:  u64,
    pub node:     String,
    pub claims:   Vec<Claim>,
    #[serde(default)]
    pub handlers: Vec<Handler>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SignedBundle {
    pub bundle:    Bundle,
    pub signer_pk: String,   // hex ed25519 public key
    pub signature: String,   // hex ed25519 sig over canonical JSON of bundle
}

// ─── Agent-internal per-claim state ────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Health {
    Healthy,
    Degraded(String),
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Backup {
    /// If true, the resource existed before we touched it.
    pub existed:       bool,
    /// For File claims: sha256 + content bytes (base64) of prior content.
    pub prior_content: Option<String>,
    pub prior_hash:    Option<String>,
    pub prior_mode:    Option<u32>,
    /// For SystemdUnit: prior active/enabled state.
    pub prior_active:  Option<bool>,
    pub prior_enabled: Option<bool>,
}

/// What a Provider's `capture` returns. Persisted forever once written;
/// never recomputed for a given claim id. The honest-unapply machinery
/// reads this — never re-derives — when reversing a mutation.
///
/// For the design rationale (capture-once-at-first-touch), see DESIGN.md §6.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Capture {
    /// Did this resource exist before Golem ever touched this claim?
    pub preexisting: bool,
    /// Provider-specific backup data needed to restore prior state.
    pub backup:      Backup,
}

/// Hard limit on a single claim's capture size. File contents that would
/// exceed this cause `capture` to return `CaptureError::TooLarge` and the
/// engine refuses the claim — surfacing it in `/status` rather than
/// silently OOM-ing the agent.
pub const MAX_CAPTURE_BYTES: usize = 1 << 20; // 1 MiB

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("capture exceeds {MAX_CAPTURE_BYTES} bytes (got {0})")]
    TooLarge(usize),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClaimState {
    pub id:               ClaimId,
    /// True once `capture` has run and been persisted for this claim. Capture
    /// is one-shot per claim id; never re-derived. See DESIGN.md §6.
    #[serde(default)]
    pub captured:         bool,
    /// Set at capture time, never recomputed. Mirror of `capture.preexisting`,
    /// kept on ClaimState for cheap dispatch in unmutate.
    pub preexisting:      bool,
    /// Provider-specific captured prior state.
    pub backup:           Backup,
    pub last_applied:     Option<DateTime<Utc>>,
    pub last_health:      Option<Health>,
    /// For File-like claims: hash of what we last wrote. Lets us detect
    /// "file didn't change, no handlers need firing."
    pub content_hash:     Option<String>,
    /// The last spec we applied. Stored so orphan-unapply can run without
    /// needing the (now-departed) bundle to still hold the claim.
    #[serde(default)]
    pub last_spec:        Option<ClaimSpec>,
}

impl ClaimState {
    pub fn fresh(id: ClaimId) -> Self {
        Self {
            id,
            captured:        false,
            preexisting:     false,
            backup:          Backup::default(),
            last_applied:    None,
            last_health:     None,
            content_hash:    None,
            last_spec:       None,
        }
    }

    /// Reconstruct a Capture from the persisted ClaimState fields.
    pub fn capture(&self) -> Capture {
        Capture {
            preexisting: self.preexisting,
            backup:      self.backup.clone(),
        }
    }
}
