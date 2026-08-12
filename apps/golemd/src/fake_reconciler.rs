//! An in-memory [`Reconciler`] that tracks each glyph's key and content id
//! without touching the host — the default golemd runs under (`--reconciler
//! fake`) and the one the foreman's diff/enact/journal spine is tested against.
//! It records what it would do; it never installs, writes, or signs anything.

use scroll_format::{ContentId, Entry, Glyph};
use std::collections::BTreeMap;
use std::sync::Mutex;
use tracing::info;

use crate::journal::{GlyphOp, Outcome};
use crate::observe::{Observation, Observations, Unknowable};
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

    /// Seed the fake's record as if `key` were already on the host at `cid`,
    /// bypassing `apply` — the only way to make this fake disagree with
    /// itself. `present` is golem's own memory of what it applied; without a
    /// seam like this the plan's two columns could never diverge under
    /// `--reconciler fake`, leaving the join/summary/render code — where
    /// ADR 0058's actual risk lives — a harness that only ever exercises the
    /// happy path.
    pub fn preexisting(self, key: &str, cid: ContentId) -> Self {
        self.present.lock().unwrap().insert(key.to_string(), cid);
        self
    }

    /// The other half of [`Self::preexisting`]: make `key` observe as gone
    /// even though the fake itself applied it, modelling a host where
    /// something golem installed was since removed out of band.
    pub fn vanished(self, key: &str) -> Self {
        self.present.lock().unwrap().remove(key);
        self
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

    /// Mirrors [`crate::reconcilers::HostReconciler::observe`]'s verdicts
    /// using `present` as the stand-in host: `Absent` if the key was never
    /// recorded, `Realized` at the matching content id, `Divergent`
    /// otherwise — except a `Remove`, which is asked presence rather than
    /// equality (any recorded id counts), the same weaker question the real
    /// reconciler asks. `openable` runs first, so a glyph this fake's
    /// keyring cannot open reports `Unknown(Sealed)` before `present` is
    /// even consulted.
    fn observe(&self, ops: &[GlyphOp]) -> Observations {
        let present = self.present.lock().unwrap();
        ops.iter()
            .map(|op| {
                let key = op.key();
                let verdict = if self.openable(op.glyph()).is_err() {
                    Observation::Unknown(Unknowable::Sealed)
                } else {
                    match op {
                        GlyphOp::Remove { .. } => match present.get(&key) {
                            None => Observation::Absent,
                            Some(_) => Observation::Realized,
                        },
                        GlyphOp::Install { cid, .. } | GlyphOp::Noop { cid, .. } => {
                            match present.get(&key) {
                                None => Observation::Absent,
                                Some(held) if held == cid => Observation::Realized,
                                Some(_) => Observation::Divergent,
                            }
                        }
                        GlyphOp::Replace { new_cid, .. } => match present.get(&key) {
                            None => Observation::Absent,
                            Some(held) if held == new_cid => Observation::Realized,
                            Some(_) => Observation::Divergent,
                        },
                    }
                };
                (key, verdict)
            })
            .collect()
    }

    // NOTE: the log lines name no `systemctl` verb. The real reconciler picks
    // between `try-restart` and `restart` (and `try-reload-or-restart` and
    // `reload-or-restart`) from the unit's failed state, which this fake does not
    // model — naming either one here would put a verb in the log that the host
    // may not have issued.
    fn restart_unit(&self, unit: &str) -> EnactResult<()> {
        info!(unit = %unit, "restart unit");
        Ok(())
    }

    fn try_reload_or_restart(&self, unit: &str) -> EnactResult<()> {
        info!(unit = %unit, "reload-or-restart unit");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scroll_format::{Chunk, Perms, Secret, Text};

    fn perms() -> Perms {
        Perms {
            mode: 0o644,
            owner: None,
            group: None,
        }
    }

    fn file_glyph(path: &str, contents: &str) -> Glyph {
        Glyph::Filesystem {
            path: path.to_string(),
            entry: Entry::File {
                contents: contents.into(),
                perms: perms(),
            },
        }
    }

    fn sealed_file_glyph(path: &str) -> Glyph {
        Glyph::Filesystem {
            path: path.to_string(),
            entry: Entry::File {
                contents: Text::composed(vec![Chunk::Hole(Secret::Sealed {
                    key_id: "6fb6c6005355abf3".to_string(),
                    ciphertext: vec![0; 32],
                })]),
                perms: perms(),
            },
        }
    }

    fn install_op(glyph: &Glyph) -> GlyphOp {
        GlyphOp::Install {
            cid: crate::reconcile::glyph_content_id(glyph),
            glyph: glyph.clone(),
        }
    }

    fn remove_op(glyph: &Glyph) -> GlyphOp {
        GlyphOp::Remove {
            cid: crate::reconcile::glyph_content_id(glyph),
            glyph: glyph.clone(),
        }
    }

    #[test]
    fn a_glyph_the_fake_applied_observes_as_realized() {
        let glyph = file_glyph("/etc/motd", "hello\n");
        let cid = crate::reconcile::glyph_content_id(&glyph);
        let fake = FakeReconciler::new();
        fake.apply(&glyph, cid).unwrap();
        let ops = vec![install_op(&glyph)];

        assert_eq!(fake.observe(&ops).get(&glyph.key()), Observation::Realized);
    }

    #[test]
    fn a_glyph_the_fake_never_applied_observes_as_absent() {
        let glyph = file_glyph("/etc/motd", "hello\n");
        let fake = FakeReconciler::new();
        let ops = vec![install_op(&glyph)];

        assert_eq!(fake.observe(&ops).get(&glyph.key()), Observation::Absent);
    }

    #[test]
    fn a_preexisting_glyph_at_a_different_cid_observes_as_divergent() {
        let desired = file_glyph("/etc/motd", "hello\n");
        let on_host = file_glyph("/etc/motd", "ansible wrote this\n");
        let desired_cid = crate::reconcile::glyph_content_id(&desired);
        let host_cid = crate::reconcile::glyph_content_id(&on_host);
        let fake = FakeReconciler::new().preexisting(&desired.key(), host_cid);
        let ops = vec![GlyphOp::Install {
            cid: desired_cid,
            glyph: desired.clone(),
        }];

        assert_eq!(
            fake.observe(&ops).get(&desired.key()),
            Observation::Divergent
        );
    }

    #[test]
    fn a_preexisting_glyph_the_journal_never_saw_observes_as_realized() {
        let glyph = file_glyph("/etc/motd", "hello\n");
        let cid = crate::reconcile::glyph_content_id(&glyph);
        let fake = FakeReconciler::new().preexisting(&glyph.key(), cid);
        let ops = vec![GlyphOp::Install {
            cid,
            glyph: glyph.clone(),
        }];

        assert_eq!(fake.observe(&ops).get(&glyph.key()), Observation::Realized);
    }

    #[test]
    fn a_vanished_glyph_observes_as_absent_though_the_fake_applied_it() {
        let glyph = file_glyph("/etc/motd", "hello\n");
        let cid = crate::reconcile::glyph_content_id(&glyph);
        let fake = FakeReconciler::new();
        fake.apply(&glyph, cid).unwrap();
        let fake = fake.vanished(&glyph.key());
        let ops = vec![install_op(&glyph)];

        assert_eq!(fake.observe(&ops).get(&glyph.key()), Observation::Absent);
    }

    #[test]
    fn a_remove_of_a_glyph_still_on_the_fake_host_observes_as_realized() {
        let glyph = file_glyph("/etc/motd", "hello\n");
        let cid = crate::reconcile::glyph_content_id(&glyph);
        let fake = FakeReconciler::new().preexisting(&glyph.key(), cid);
        let ops = vec![remove_op(&glyph)];

        assert_eq!(fake.observe(&ops).get(&glyph.key()), Observation::Realized);
    }

    #[test]
    fn a_remove_of_a_glyph_already_gone_observes_as_absent() {
        let glyph = file_glyph("/etc/motd", "hello\n");
        let fake = FakeReconciler::new();
        let ops = vec![remove_op(&glyph)];

        assert_eq!(fake.observe(&ops).get(&glyph.key()), Observation::Absent);
    }

    #[test]
    fn a_sealed_glyph_this_fake_cannot_open_observes_as_unknown_sealed() {
        let glyph = sealed_file_glyph("/etc/app/creds.conf");
        let fake = FakeReconciler::new().with_keyring(Keyring::without_key());
        let ops = vec![install_op(&glyph)];

        assert_eq!(
            fake.observe(&ops).get(&glyph.key()),
            Observation::Unknown(Unknowable::Sealed)
        );
    }
}
