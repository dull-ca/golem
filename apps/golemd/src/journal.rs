//! golemd's memory of its own edits: the append-only journal types and the
//! last-applied state it stores.
//!
//! Reversibility is the load-bearing property here (ADR 0015). The host cannot
//! answer "did golem add this line, or was it already here?"; the journal is
//! the only reliable record of what golem changed. So every applied glyph
//! carries its [`Inverse`] — captured at apply time — and golem only ever
//! reverses edits it recorded, never touching pre-existing host state.

use chrono::{DateTime, Utc};
use scroll_format::{ContentId, Glyph, Scroll};
use serde::{Deserialize, Serialize};

/// A single glyph operation the pure diff (`reconcile::plan`) decided on for one
/// resource, keyed by [`Glyph::key`] and versioned by content id (ADR 0015 §2):
/// `Install` a new key, `Remove` a key that left the desired scroll, `Replace`
/// when the key stayed but its content id changed (an upgrade), `Noop` when the
/// content id matched.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum GlyphOp {
    Install { cid: ContentId, glyph: Glyph },
    Remove { cid: ContentId, glyph: Glyph },
    Replace { old_cid: ContentId, new_cid: ContentId, glyph: Glyph },
    Noop { cid: ContentId, glyph: Glyph },
}

impl GlyphOp {
    pub fn key(&self) -> String {
        self.glyph().key()
    }

    pub fn glyph(&self) -> &Glyph {
        match self {
            GlyphOp::Install { glyph, .. }
            | GlyphOp::Remove { glyph, .. }
            | GlyphOp::Replace { glyph, .. }
            | GlyphOp::Noop { glyph, .. } => glyph,
        }
    }
}

/// The prior host state to restore when an applied glyph is reversed —
/// captured at apply time, carrying exactly and only what apply changed (ADR
/// 0015 §1). Each variant restores one primitive:
///
/// - [`Nothing`](Inverse::Nothing) — golem did not change the host (the glyph
///   already matched), so reverse is a no-op. Also the inverse of a `Noop`.
/// - [`RemoveAptPackage`](Inverse::RemoveAptPackage) — golem installed the
///   package, so reverse removes it. (A package already present at apply time
///   records `Nothing` and is left alone.)
/// - [`DisableSystemdService`](Inverse::DisableSystemdService) — records the
///   unit's `prior_enabled`/`prior_active` at apply time; reverse restores
///   exactly that (disable if golem enabled it, stop if golem started it, else
///   leave it — see `reconcilers::reverse_systemd`).
/// - [`RestoreFile`](Inverse::RestoreFile) — golem overwrote an existing file;
///   reverse rewrites the prior contents+mode.
/// - [`DeleteFile`](Inverse::DeleteFile) — golem created the file; reverse
///   deletes it.
/// - [`RemoveLineInFile`](Inverse::RemoveLineInFile) — golem appended the line;
///   reverse removes the first matching occurrence it added.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Inverse {
    Nothing,
    RemoveAptPackage { name: String },
    DisableSystemdService { unit: String, prior_enabled: bool, prior_active: bool, started_only: bool },
    RestoreFile { path: String, contents: String, mode: String },
    DeleteFile { path: String },
    RemoveLineInFile { path: String, line: String },
}

/// What a reconciler's `apply` returns and the journal stores per glyph: the
/// operation performed, its content id (the versioning axis), the [`Inverse`]
/// to undo it, and `changed = false` when the host already matched (an
/// idempotent no-op). The ordered list of these on a [`Revision`] is golem's
/// reversal record — reversed LIFO for a rollback or a later `Remove`/`Replace`
/// (ADR 0015 §1/§3).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Outcome {
    pub op: GlyphOp,
    pub cid: ContentId,
    pub inverse: Inverse,
    pub changed: bool,
}

/// Only two journal-entry kinds (ADR 0014 §4): `Init`, written once when the
/// plan room is first opened, and `Reconcile` for every applied manifest. There
/// is no decommission kind — removing everything is reconciling toward an empty
/// scroll.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RevisionKind {
    Init,
    Reconcile,
}

/// One append-only journal entry: the scroll's content id enacted and the
/// ordered [`Outcome`]s (the reversal record). `scroll_content_id` is `None`
/// only for the initial `Init` entry.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Revision {
    pub id: u64,
    pub created_at: DateTime<Utc>,
    pub kind: RevisionKind,
    pub scroll_content_id: Option<ContentId>,
    pub outcomes: Vec<Outcome>,
}

/// The last manifest this node accepted: the applied scroll, its content id,
/// and the [`Outcome`]s that enacted it. `reconcile::plan` diffs the next
/// desired scroll against these `outcomes` to decide each glyph's operation.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppliedState {
    pub scroll_content_id: ContentId,
    pub scroll: Scroll,
    pub outcomes: Vec<Outcome>,
}
