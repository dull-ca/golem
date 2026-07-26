//! The `Reconciler` port: the one narrow interface the reconcile spine calls to
//! enact a glyph and to undo it (ADR 0014 §3, ADR 0015 §1). It speaks glyph
//! vocabulary — `apply`/`reverse` over a `Glyph` — never apt or systemd; the
//! host adapters live in `reconcilers.rs`, the in-memory fake in
//! `fake_reconciler.rs`.

use scroll_format::{ContentId, Entry, Glyph};
use std::sync::Arc;

use crate::journal::{Inverse, Outcome};

/// Why an enact step failed, and whether retrying could help: `Retryable` is
/// retried by the foreman's attempt spine, `Fatal` aborts the reconcile at once.
#[derive(Debug)]
pub enum EnactError {
    Retryable(String),
    Fatal(String),
}

pub type EnactResult<T> = Result<T, EnactError>;

/// Enact one glyph and record how to reverse it. `apply` brings the host to
/// `glyph` and returns the [`Outcome`] receipt — the content id, the captured
/// [`Inverse`], and whether anything changed — that `reverse` later consumes to
/// restore the prior state exactly. Both are idempotent: re-applying a matching
/// glyph reports `changed = false`, and reverse only undoes what golem recorded
/// doing.
pub trait Reconciler: Send + Sync {
    fn apply(&self, glyph: &Glyph, cid: ContentId) -> EnactResult<Outcome>;
    fn reverse(&self, outcome: &Outcome) -> EnactResult<()>;
    fn restart_unit(&self, _unit: &str) -> EnactResult<()> {
        Ok(())
    }
    /// Best-effort host evidence about why a glyph could not settle, captured at
    /// give-up time before any rollback removes the trace. `None` when a kind has
    /// no diagnostics or the probe found nothing; never an error — a probe that
    /// fails yields `None` or a partial. Travels in the report, never the journal.
    fn diagnose(&self, _glyph: &Glyph) -> Option<String> {
        None
    }
}

impl<R: Reconciler + ?Sized> Reconciler for Arc<R> {
    fn apply(&self, glyph: &Glyph, cid: ContentId) -> EnactResult<Outcome> {
        (**self).apply(glyph, cid)
    }
    fn reverse(&self, outcome: &Outcome) -> EnactResult<()> {
        (**self).reverse(outcome)
    }
    fn restart_unit(&self, unit: &str) -> EnactResult<()> {
        (**self).restart_unit(unit)
    }
    fn diagnose(&self, glyph: &Glyph) -> Option<String> {
        (**self).diagnose(glyph)
    }
}

/// The default [`Inverse`] for a glyph when no prior host state was captured —
/// the receipt the fake reconciler and the foreman's synthesized
/// `prior_outcome` use. It assumes golem added the glyph, so reverse removes it;
/// the real host reconcilers override this with the actual prior state observed
/// at apply time.
pub fn inverse_of(glyph: &Glyph) -> Inverse {
    match glyph {
        Glyph::AptPackage { name } => Inverse::RemoveAptPackage { name: name.clone() },
        Glyph::SystemdService { unit } => Inverse::DisableSystemdService {
            unit: unit.clone(),
            prior_enabled: false,
            prior_active: false,
            started_only: false,
        },
        Glyph::Filesystem { path, entry } => match entry {
            Entry::File { .. } => Inverse::DeleteFile { path: path.clone() },
            Entry::Directory { .. } => Inverse::RemoveDirectory {
                path: path.clone(),
                created: vec![path.clone()],
            },
            Entry::Symlink { .. } => Inverse::RemoveSymlink { path: path.clone() },
        },
        Glyph::LineInFile { path, line } => Inverse::RemoveLineInFile {
            path: path.clone(),
            line: line.clone(),
        },
    }
}
