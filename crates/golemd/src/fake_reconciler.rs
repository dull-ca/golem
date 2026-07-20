//! An in-memory [`Reconciler`] that tracks each glyph's key and content id
//! without touching the host — the default golemd runs under (`--reconciler
//! fake`) and the one the foreman's diff/enact/journal spine is tested against.
//! It records what it would do; it never installs, writes, or signs anything.

use scroll_format::{ContentId, Glyph};
use std::collections::BTreeMap;
use std::sync::Mutex;
use tracing::info;

use crate::journal::{GlyphOp, Outcome};
use crate::reconciler::{inverse_of, EnactResult, Reconciler};

/// Remembers the content id last applied per glyph key, so `apply` reports
/// `changed = false` when re-applying the same id — the same idempotence the
/// real reconcilers give, with no side effects.
#[derive(Default)]
pub struct FakeReconciler {
    present: Mutex<BTreeMap<String, ContentId>>,
}

impl FakeReconciler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn present_keys(&self) -> Vec<String> {
        self.present.lock().unwrap().keys().cloned().collect()
    }
}

impl Reconciler for FakeReconciler {
    fn apply(&self, glyph: &Glyph, cid: ContentId) -> EnactResult<Outcome> {
        let key = glyph.key();
        info!(key = %key, "apply glyph");
        let mut present = self.present.lock().unwrap();
        let changed = present.get(&key) != Some(&cid);
        present.insert(key, cid);
        Ok(Outcome {
            op: GlyphOp::Install { cid, glyph: glyph.clone() },
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
}
