//! golemd's memory of its own edits: the append-only journal types and the
//! last-applied state it stores.
//!
//! Reversibility is the load-bearing property here (ADR 0015). The host cannot
//! answer "did golem add this line, or was it already here?"; the journal is
//! the only reliable record of what golem changed. So every applied glyph
//! carries its [`Inverse`] — captured at apply time — and golem only ever
//! reverses edits it recorded, never touching pre-existing host state.

use chrono::{DateTime, Utc};
use scroll_format::{ContentId, Glyph, Perms, Scroll};
use serde::{Deserialize, Serialize};

/// A single glyph operation the pure diff (`reconcile::plan`) decided on for one
/// resource, keyed by [`Glyph::key`] and versioned by content id (ADR 0015 §2):
/// `Install` a new key, `Remove` a key that left the desired scroll, `Replace`
/// when the key stayed but its content id changed (an upgrade), `Noop` when the
/// content id matched.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum GlyphOp {
    Install {
        cid: ContentId,
        glyph: Glyph,
    },
    Remove {
        cid: ContentId,
        glyph: Glyph,
    },
    Replace {
        old_cid: ContentId,
        new_cid: ContentId,
        glyph: Glyph,
    },
    Noop {
        cid: ContentId,
        glyph: Glyph,
    },
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
///   unit's `prior_enabled`/`prior_active` at apply time, plus `started_only`:
///   set when `enable` was refused (a generated/quadlet unit) and golem could
///   only `start` the unit. Reverse of a `started_only` unit *stops* it and
///   never disables it — golem never enabled it. Otherwise reverse restores the
///   recorded prior enabled/active state (disable if golem enabled it, stop if
///   golem started an inactive unit, else leave it — see
///   `reconcilers::reverse_systemd`).
/// - [`RestoreFile`](Inverse::RestoreFile) — golem overwrote an existing file;
///   reverse rewrites the prior contents and [`Perms`] (mode + ownership — the
///   filesystem glyph carries ownership now, ADR 0019, so the inverse records it).
/// - [`DeleteFile`](Inverse::DeleteFile) — golem created the file; reverse
///   deletes it.
/// - [`RemoveDirectory`](Inverse::RemoveDirectory) — golem created the
///   directory. `created` is the ordered list of components `create_dir_all`
///   actually made, deepest-first; reverse `rmdir`s them in that order and stops
///   at the first non-empty one, so golem removes only the empty directories it
///   created and never a component a later glyph or a container populated
///   (`reconcilers::remove_directory`). Never records more than golem made, or
///   reverse would over-delete (ADR 0019 §4).
/// - [`RestoreDirMeta`](Inverse::RestoreDirMeta) — the directory pre-existed and
///   golem only changed its perms/ownership; reverse restores `prior_perms` and
///   never removes the directory.
/// - [`RemoveSymlink`](Inverse::RemoveSymlink) — golem created the symlink;
///   reverse `unlink`s it. (A symlink golem did not create is never touched —
///   apply refuses to clobber a pre-existing entry rather than recording one.)
/// - [`RemoveLineInFile`](Inverse::RemoveLineInFile) — golem appended the line;
///   reverse removes the first matching occurrence it added.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Inverse {
    Nothing,
    RemoveAptPackage {
        name: String,
    },
    DisableSystemdService {
        unit: String,
        prior_enabled: bool,
        prior_active: bool,
        started_only: bool,
    },
    RestoreFile {
        path: String,
        contents: String,
        perms: Perms,
    },
    DeleteFile {
        path: String,
    },
    RemoveDirectory {
        path: String,
        created: Vec<String>,
    },
    RestoreDirMeta {
        path: String,
        prior_perms: Perms,
    },
    RemoveSymlink {
        path: String,
    },
    RemoveLineInFile {
        path: String,
        line: String,
    },
}

/// What a reconciler's `apply` returns and golem records per glyph: the
/// operation performed, its content id (the versioning axis), the [`Inverse`]
/// to undo it, and `changed = false` when the host already matched (an
/// idempotent no-op) (ADR 0015 §1). Two roles: an `Outcome` is embedded in each
/// [`WalStep`]'s `Done` row (the durable receipt), and the currently-applied set
/// is a list of `Outcome`s folded out of the WAL (`wal::applied_outcomes`) —
/// what `reconcile::plan` diffs the next scroll against. The [`Revision`] carries
/// that same folded list as its projection of the attempt.
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

/// A read cache of the last manifest this node accepted: the applied scroll, its
/// content id, and the [`Outcome`]s that enacted it. Under ADR 0020 this is no
/// longer authoritative — the applied set is the fold of the WAL
/// (`wal::applied_outcomes`), and this row is rebuilt from it. The `scroll`/
/// `scroll_content_id` fields, which the WAL does not carry, are the reason the
/// cache is kept at all.
// NOTE: no `Eq` — this embeds `Scroll`, whose `Policy` carries `f64` knobs and so
// is only `PartialEq` (scroll-format `scroll.rs`, ADR 0031 §3). Don't add `Eq`
// back.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AppliedState {
    pub scroll_content_id: ContentId,
    pub scroll: Scroll,
    pub outcomes: Vec<Outcome>,
}

/// Which direction a [`WalStep`] drives the host (ADR 0020 §2): `Apply` brings a
/// glyph to its target, `Reverse` undoes a prior `Apply` from its captured
/// [`Inverse`]. A `Replace` that cannot update in place is recorded as a
/// `Reverse` of the old version followed by an `Apply` of the new one; a rollback
/// is a sequence of steps whose `action` is the opposite of the one it undoes.
///
/// `Restart` and `Reload` are non-fold directions: the config-propagation pass
/// (`foreman::propagate_config`) records each `try-restart` of a unit whose
/// backing file changed (ADR 0020 §5) as a `Restart` step, and each
/// `try-reload-or-restart` of a unit an authored `notifies` named (ADR 0036) as a
/// `Reload` step. They are operational records, not claims on the applied set — a
/// restart of a running unit has no separate reversal, the unit's lifecycle stays
/// owned by its `systemdService` step. So both are deliberately excluded from
/// every fold that derives host truth (`wal::applied_outcomes`) and from rollback
/// (`foreman::next_reversible`); a crash mid-propagation re-runs the idempotent
/// systemctl call rather than reversing it (`foreman::redrive_intended`), which is
/// what keeps the two directions distinct actions rather than one action with a
/// key prefix — recovery re-drives each through the right reconciler call.
///
// NOTE: this enum is persisted as its serde *token string* in the
// `wal_step.action` column (`planroom::action_token` / `row_to_wal_step`), never
// as a postcard variant index. So appending a variant cannot misparse an older
// journal — it simply never holds the new token — but RENAMING or removing one
// silently breaks every row already written. Same scheme for `AttemptPhase` and
// `WalStepState`.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WalAction {
    Apply,
    Reverse,
    Restart,
    Reload,
}

/// The lifecycle of one WAL step (ADR 0020 §1). A step is appended `Intended`
/// before the reconciler is called and gains a terminal row after it returns:
///
/// - `Intended` — golem is about to touch the host but has not yet. An
///   `Intended` row with no later terminal row for the same step is the recovery
///   signal: the process died across the reconciler call and may or may not have
///   performed the effect.
/// - `Done` — the reconciler returned `Ok`; the row carries the captured
///   [`Inverse`] and `changed`. A `Done` with no later `Reversed` for the same
///   step is *currently applied*.
/// - `Failed` — the reconciler gave up or hit a fatal error; no lasting host
///   change is claimed (reconcilers observe host state first, so a partially-run
///   `Failed` step is safe to re-drive or reverse).
/// - `Reversed` — a previously `Done` step was later undone, by in-attempt
///   rollback or a subsequent attempt's `Remove`/`Replace`. A step is never
///   `Reversed` twice, which is what lets a rollback resume after a crash.
///
/// The store never `UPDATE`s a step; each transition is a new row (see
/// [`WalStep`]), so the sequence of rows for a step *is* its history.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WalStepState {
    Intended,
    Done,
    Failed,
    Reversed,
}

/// Where a [`ReconcileAttempt`] is in its lifecycle (ADR 0020 §2). The phase
/// advances `Planning` → `Enacting` → (`Committed` | `RollingBack` →
/// `RolledBack`). Recovery reads it to decide what an interrupted attempt needs:
/// an `Enacting` attempt is rolled back, a `RollingBack` one is resumed. Ingest
/// of a new manifest is gated on the latest attempt being settled.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AttemptPhase {
    Planning,
    Enacting,
    RollingBack,
    Committed,
    RolledBack,
}

impl AttemptPhase {
    /// A settled attempt is finished: nothing about it is in flight, so recovery
    /// leaves it alone and a new reconcile may open. Only the two terminal
    /// phases settle; `Planning`/`Enacting`/`RollingBack` are all in-progress.
    pub fn is_settled(self) -> bool {
        matches!(self, AttemptPhase::Committed | AttemptPhase::RolledBack)
    }
}

/// One reconcile toward a scroll — the frame every [`WalStep`] belongs to (ADR
/// 0020 §2). Opened before planning and closed once its steps settle. A committed
/// attempt *is* the `Reconcile` [`Revision`] projected from the WAL, so an
/// attempt and its revision are one durable record, not two writes that can
/// diverge across a crash.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReconcileAttempt {
    pub reconcile_id: u64,
    pub started_at: DateTime<Utc>,
    pub scroll_content_id: Option<ContentId>,
    pub phase: AttemptPhase,
    pub settled_at: Option<DateTime<Utc>>,
}

/// One transition of one glyph op within a [`ReconcileAttempt`] (ADR 0020 §2).
/// Append-only: a state change is a new row with a higher `seq`, so a step's
/// history is its rows in `seq` order. `step_ord` is the op's position in the
/// plan and, with `action`, identifies which step a later row transitions —
/// `seq` orders rows, `step_ord`+`action` groups them. `inverse` holds the state
/// a `Reverse` intends to consume or the state an `Apply` captured; `changed` is
/// populated once the step is `Done`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WalStep {
    pub seq: u64,
    pub reconcile_id: u64,
    pub step_ord: u64,
    pub glyph_key: String,
    pub action: WalAction,
    pub state: WalStepState,
    pub op: GlyphOp,
    pub inverse: Option<Inverse>,
    pub changed: Option<bool>,
    /// Root-to-leaf name-path of the leaf unit this op belongs to (ADR 0031 §6),
    /// a vanished unit's ops carrying its surviving parent's path (§4). Additive
    /// and for reporting only — the recovery fold carries it but does not consult
    /// it, so the bracketing invariant and `step_ord`+`action` grouping are
    /// unchanged.
    pub unit_path: Vec<String>,
    pub at: DateTime<Utc>,
}
