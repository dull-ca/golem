//! An in-memory [`Reconciler`] that tracks each glyph's key and content id
//! without touching the host — the default golemd runs under (`--reconciler
//! fake`) and the one the foreman's diff/enact/journal spine is tested against.
//! It records what it would do; it never installs, writes, or signs anything.

use scroll_format::{ContentId, Entry, Glyph};
use std::collections::BTreeMap;
use std::sync::Mutex;
use tracing::info;

use crate::journal::{GlyphOp, Outcome};
use crate::reconciler::{inverse_of, EnactResult, Reconciler};
use crate::secrets::Keyring;

/// Remembers the content id last applied per glyph key, so `apply` reports
/// `changed = false` when re-applying the same id — the same idempotence the
/// real reconcilers give, with no side effects.
#[derive(Default)]
pub struct FakeReconciler {
    present: Mutex<BTreeMap<String, ContentId>>,
    keyring: Keyring,
}

impl FakeReconciler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_keyring(mut self, keyring: Keyring) -> Self {
        self.keyring = keyring;
        self
    }

    pub fn present_keys(&self) -> Vec<String> {
        self.present.lock().unwrap().keys().cloned().collect()
    }

    /// Refuse a glyph the configured key cannot open, exactly as the host
    /// adapters would, then drop the plaintext unread — the fake records intent
    /// and writes nothing, so it has no use for the value itself. Without this a
    /// `--reconciler fake` dry run would report a settled apply for a manifest
    /// the same host could not enact.
    fn openable(&self, glyph: &Glyph) -> EnactResult<()> {
        let text = match glyph {
            Glyph::Filesystem {
                entry: Entry::File { contents, .. },
                ..
            } => contents,
            Glyph::LineInFile { line, .. } => line,
            Glyph::AptPackage { .. } | Glyph::SystemdService { .. } | Glyph::Filesystem { .. } => {
                return Ok(())
            }
        };
        self.keyring.open(text, &glyph.key()).map(drop)
    }
}

impl Reconciler for FakeReconciler {
    fn apply(&self, glyph: &Glyph, cid: ContentId) -> EnactResult<Outcome> {
        self.openable(glyph)?;
        let key = glyph.key();
        info!(key = %key, "apply glyph");
        let mut present = self.present.lock().unwrap();
        let changed = present.get(&key) != Some(&cid);
        present.insert(key, cid);
        Ok(Outcome {
            op: GlyphOp::Install {
                cid,
                glyph: glyph.clone(),
            },
            cid,
            inverse: inverse_of(glyph),
            changed,
        })
    }

    fn reverse(&self, outcome: &Outcome) -> EnactResult<()> {
        let key = outcome.op.key();
        info!(key = %key, "reverse glyph");
        self.present.lock().unwrap().remove(&key);
        Ok(())
    }

    fn restart_unit(&self, unit: &str) -> EnactResult<()> {
        info!(unit = %unit, "try-restart unit");
        Ok(())
    }

    fn try_reload_or_restart(&self, unit: &str) -> EnactResult<()> {
        info!(unit = %unit, "try-reload-or-restart unit");
        Ok(())
    }
}
