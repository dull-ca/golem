//! Teardown of a glyph several units declare, and of the directory they share.
//!
//! The fixture is `apps/fleet/nftables-proof.emet` reproduced in-process: two
//! ingress units, each carrying the whole `Nftables.nftablesBase` (the package,
//! `/etc/nftables.d`, the entrypoint conf, the base drop-in, the unit file, the
//! service) plus one drop-in of its own, and each notifying
//! `golem-nftables.service`. Applying that manifest and then an empty scroll is
//! the smallest program that exercises every seam ADR 0041's drop-in model
//! leans on — dedup by content id (ADR 0034 §1), the notify reload (ADR 0036),
//! and the removes phase (ADR 0031 §4).
//!
//! Run against the VM fleet on 2026-07-31 it left files, a directory, and a
//! failed reload behind. Three separate golemd bugs, each pinned here:
//!
//! - the crediting unit's bare re-observation displaced the enacting unit's
//!   receipt in the applied-set fold, so `Remove` reversed an `Inverse::Nothing`
//!   and the file stayed on disk;
//! - the end-of-apply reload poked `golem-nftables.service` after the same
//!   reconcile had removed its unit file, turning a clean teardown `partial`;
//! - `/etc/nftables.d` was `rmdir`ed while a sibling unit's drop-in was still
//!   inside it, so the directory survived as an orphan.
//!
//! The `Ledger` reconciler models exactly the host behavior those bugs turn on:
//! idempotent applies that report `changed = false`, a `RemoveDirectory` that
//! declines to remove a directory still holding entries, and a `poke` that
//! fails `Unit not found` for a unit no longer present.

use std::sync::{Arc, Mutex};

use golemd::config::{EnactConfig, RetryConfig};
use golemd::foreman::Foreman;
use golemd::journal::{GlyphOp, Inverse, Outcome};
use golemd::planroom::{MemoryPlanRoom, PlanRoom};
use golemd::reconciler::{EnactError, EnactResult, Reconciler};
use golemd::report::{TopOutcome, UnitOutcome};
use golemd::wal::applied_outcomes;
use scroll_format::{ContentId, Contents, Entry, Glyph, Manifest, Perms, Scroll};

#[derive(Default)]
struct Ledger {
    present: Mutex<std::collections::BTreeMap<String, String>>,
    pokes: Mutex<Vec<String>>,
}

impl Ledger {
    fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn present(&self) -> Vec<String> {
        self.present.lock().unwrap().keys().cloned().collect()
    }

    fn seed(&self, key: &str, body: &str) {
        self.present.lock().unwrap().insert(key.into(), body.into());
    }

    fn pokes(&self) -> Vec<String> {
        self.pokes.lock().unwrap().clone()
    }

    fn holds_entries_under(&self, path: &str) -> bool {
        let prefix = format!("file:{path}/");
        self.present
            .lock()
            .unwrap()
            .keys()
            .any(|key| key.starts_with(&prefix))
    }

    fn poke(&self, verb: &str, unit: &str) -> EnactResult<()> {
        self.pokes.lock().unwrap().push(format!("{verb}:{unit}"));
        if self
            .present
            .lock()
            .unwrap()
            .contains_key(&format!("systemd:{unit}"))
        {
            return Ok(());
        }
        Err(EnactError::Retryable(format!(
            "systemctl {verb} {unit}: Failed to {verb} {unit}: Unit {unit} not found."
        )))
    }
}

fn body_of(glyph: &Glyph) -> String {
    match glyph {
        Glyph::AptPackage { name } => name.clone(),
        Glyph::SystemdService { unit } => unit.clone(),
        Glyph::Filesystem {
            entry: Entry::File { contents, .. },
            ..
        } => contents.to_string(),
        other => format!("{other:?}"),
    }
}

impl Reconciler for Ledger {
    fn apply(&self, glyph: &Glyph, cid: ContentId) -> EnactResult<Outcome> {
        let key = glyph.key();
        let body = body_of(glyph);
        let op = GlyphOp::Install {
            cid,
            glyph: glyph.clone(),
        };
        let prior = self.present.lock().unwrap().get(&key).cloned();
        if prior.as_deref() == Some(body.as_str()) {
            return Ok(Outcome {
                op,
                cid,
                inverse: Inverse::Nothing,
                changed: false,
            });
        }
        self.present.lock().unwrap().insert(key, body);
        Ok(Outcome {
            op,
            cid,
            inverse: golemd::reconciler::inverse_of(glyph),
            changed: true,
        })
    }

    fn reverse(&self, outcome: &Outcome) -> EnactResult<()> {
        let key = outcome.op.key();
        match &outcome.inverse {
            Inverse::Nothing => Ok(()),
            Inverse::DeleteFile { .. }
            | Inverse::RemoveAptPackage { .. }
            | Inverse::DisableSystemdService { .. } => {
                self.present.lock().unwrap().remove(&key);
                Ok(())
            }
            Inverse::RemoveDirectory { path, .. } => {
                if self.holds_entries_under(path) {
                    return Ok(());
                }
                self.present.lock().unwrap().remove(&key);
                Ok(())
            }
            other => Err(EnactError::Fatal(format!("unexpected inverse {other:?}"))),
        }
    }

    fn restart_unit(&self, unit: &str) -> EnactResult<()> {
        self.poke("try-restart", unit)
    }

    fn try_reload_or_restart(&self, unit: &str) -> EnactResult<()> {
        self.poke("try-reload-or-restart", unit)
    }
}

fn file(path: &str, contents: &str) -> Glyph {
    Glyph::Filesystem {
        path: path.into(),
        entry: Entry::File {
            contents: contents.into(),
            perms: Perms {
                mode: 0o644,
                owner: None,
                group: None,
            },
        },
    }
}

fn directory(path: &str) -> Glyph {
    Glyph::Filesystem {
        path: path.into(),
        entry: Entry::Directory {
            perms: Perms {
                mode: 0o755,
                owner: None,
                group: None,
            },
        },
    }
}

fn nftables_base() -> Vec<Glyph> {
    vec![
        Glyph::AptPackage {
            name: "nftables".into(),
        },
        file("/etc/golem-nftables.conf", "entrypoint"),
        directory("/etc/nftables.d"),
        file("/etc/nftables.d/00-base.nft", "base chain"),
        file("/etc/systemd/system/golem-nftables.service", "unit file"),
        Glyph::SystemdService {
            unit: "golem-nftables.service".into(),
        },
    ]
}

fn ingress_unit(name: &str) -> Scroll {
    let mut glyphs = vec![file(
        &format!("/etc/nftables.d/ingress-{name}.nft"),
        &format!("ingress {name}"),
    )];
    glyphs.extend(nftables_base());
    Scroll {
        name: name.into(),
        policy: None,
        notifies: vec!["golem-nftables.service".into()],
        contents: Contents::Glyphs(glyphs),
    }
}

fn two_unit_manifest() -> Vec<u8> {
    scroll_format::to_bytes(&Manifest::from_scrolls(
        vec![Scroll {
            name: "scaly".into(),
            policy: None,
            notifies: vec![],
            contents: Contents::Groups(vec![
                ingress_unit("alpha.example"),
                ingress_unit("bravo.example"),
            ]),
        }],
        "test",
    ))
}

fn empty_manifest() -> Vec<u8> {
    scroll_format::to_bytes(&Manifest::from_scrolls(
        vec![Scroll {
            name: "scaly".into(),
            policy: None,
            notifies: vec![],
            contents: Contents::Glyphs(vec![]),
        }],
        "test",
    ))
}

fn foreman_with_workers(room: Arc<MemoryPlanRoom>, rec: Arc<Ledger>, workers: usize) -> Foreman {
    Foreman::new("scaly".into(), Box::new(room), Box::new(rec))
        .with_retry_config(RetryConfig {
            max_attempts: 1,
            base_delay_ms: 0,
            ..Default::default()
        })
        .with_enact_config(EnactConfig { workers })
}

fn foreman(room: Arc<MemoryPlanRoom>, rec: Arc<Ledger>) -> Foreman {
    foreman_with_workers(room, rec, 1)
}

/// The whole story end to end, serially: apply, then decommission, and check
/// nothing golem put on the host is left.
#[test]
fn a_glyph_two_units_share_is_removed_when_the_next_scroll_drops_it() {
    let room = Arc::new(MemoryPlanRoom::new());
    let rec = Ledger::new();
    let f = foreman(room.clone(), rec.clone());

    f.apply_manifest(&two_unit_manifest()).unwrap();
    assert_eq!(
        rec.present().len(),
        8,
        "both unique ingress files plus the six shared glyphs are on the host"
    );

    f.apply_manifest(&empty_manifest()).unwrap();

    assert_eq!(
        rec.present(),
        Vec::<String>::new(),
        "an empty scroll reverses every glyph golem applied, shared ones included"
    );
    assert!(
        applied_outcomes(&room.wal_steps().unwrap()).is_empty(),
        "and the applied-set fold agrees nothing is left"
    );
}

/// The same, on the four-worker pool and repeated, because which unit enacts a
/// shared glyph and which one credits it is a race. Either the dedup set is
/// seeded first and the loser records a credited bracket, or both reach `apply`
/// and the loser's own idempotence short-circuits — two code paths writing the
/// identical bare re-observation, and the fold has to survive both.
#[test]
fn parallel_units_removing_a_shared_glyph_leave_nothing_behind() {
    for _ in 0..32 {
        let room = Arc::new(MemoryPlanRoom::new());
        let rec = Ledger::new();
        let f = foreman_with_workers(room.clone(), rec.clone(), 4);

        f.apply_manifest(&two_unit_manifest()).unwrap();
        f.apply_manifest(&empty_manifest()).unwrap();

        assert_eq!(
            rec.present(),
            Vec::<String>::new(),
            "whether a sibling credited the glyph or raced it to a real apply, \
             the receipt that undoes it survives the fold"
        );
    }
}

/// `/etc/nftables.d` holds both units' drop-ins, so its `rmdir` has to come
/// after every unit's file removes — the ordering only the trailing directory
/// wave can give it.
#[test]
fn a_directory_two_units_fill_outlives_every_file_either_one_put_in_it() {
    let room = Arc::new(MemoryPlanRoom::new());
    let rec = Ledger::new();
    let f = foreman(room.clone(), rec.clone());

    f.apply_manifest(&two_unit_manifest()).unwrap();
    let report = f.apply_manifest(&empty_manifest()).unwrap();

    assert_eq!(
        rec.present(),
        Vec::<String>::new(),
        "the shared directory comes off only after the last unit's file inside it, \
         so nothing is orphaned"
    );
    assert_eq!(
        report.outcome,
        TopOutcome::Settled,
        "and the teardown settles rather than silently leaving the directory behind"
    );
}

/// The limit of the guarantee. An operator's own file in the drop-in directory
/// is not golem's to delete, so the `rmdir` stops on the non-empty directory and
/// the reconcile still settles — golem only ever reverses edits it recorded.
#[test]
fn a_directory_holding_a_file_golem_never_recorded_survives_the_teardown() {
    let room = Arc::new(MemoryPlanRoom::new());
    let rec = Ledger::new();
    let f = foreman(room.clone(), rec.clone());

    f.apply_manifest(&two_unit_manifest()).unwrap();
    rec.seed("file:/etc/nftables.d/hand-written.nft", "not golem's");

    let report = f.apply_manifest(&empty_manifest()).unwrap();

    assert_eq!(
        rec.present(),
        vec![
            "file:/etc/nftables.d".to_string(),
            "file:/etc/nftables.d/hand-written.nft".to_string(),
        ],
        "a directory still holding content golem never recorded stays, and so does the content"
    );
    assert_eq!(
        report.outcome,
        TopOutcome::Settled,
        "leaving a non-empty directory alone is the designed outcome, not a failure"
    );
}

/// The first bug at its source: read the fold straight after the apply and
/// confirm the shared base drop-in still carries the enacting unit's real
/// `DeleteFile`, not the crediting sibling's `Nothing`.
#[test]
fn a_credited_sibling_does_not_erase_the_enacting_units_inverse() {
    let room = Arc::new(MemoryPlanRoom::new());
    let rec = Ledger::new();
    let f = foreman(room.clone(), rec.clone());

    f.apply_manifest(&two_unit_manifest()).unwrap();

    let fold = applied_outcomes(&room.wal_steps().unwrap());
    let shared = fold
        .iter()
        .find(|o| o.op.key() == "file:/etc/nftables.d/00-base.nft")
        .expect("the shared base file is applied");
    assert_eq!(
        shared.inverse,
        Inverse::DeleteFile {
            path: "/etc/nftables.d/00-base.nft".into()
        },
        "the applied set keeps the enacting unit's real inverse, not the crediting sibling's Nothing"
    );
    assert!(
        shared.changed,
        "the applied set records that golem changed the host for this glyph"
    );
}

/// The other direction of the same rule: protecting the receipt must not
/// multiply it. One host change earns one undo, however many units named the
/// glyph (ADR 0034 §1 — the unit that did the work owns it).
#[test]
fn a_shared_glyph_is_reversed_once_not_once_per_crediting_unit() {
    let room = Arc::new(MemoryPlanRoom::new());
    let rec = Ledger::new();
    let f = foreman(room.clone(), rec.clone());

    f.apply_manifest(&two_unit_manifest()).unwrap();
    f.apply_manifest(&empty_manifest()).unwrap();

    let reverses = room
        .wal_steps()
        .unwrap()
        .into_iter()
        .filter(|s| {
            s.glyph_key == "file:/etc/nftables.d/00-base.nft"
                && s.action == golemd::journal::WalAction::Reverse
                && s.state == golemd::journal::WalStepState::Done
        })
        .count();
    assert_eq!(
        reverses, 1,
        "a glyph applied once is reversed once, however many units credited it"
    );
}

/// `nftables` already on the box before golem arrived: every unit observes it
/// installed, nobody records an inverse, and the teardown leaves it. A fold that
/// invented a receipt for a shared glyph would fail here.
#[test]
fn a_preexisting_package_two_units_share_is_left_alone_on_removal() {
    let room = Arc::new(MemoryPlanRoom::new());
    let rec = Ledger::new();
    rec.seed("apt:nftables", "nftables");
    let f = foreman(room.clone(), rec.clone());

    f.apply_manifest(&two_unit_manifest()).unwrap();
    f.apply_manifest(&empty_manifest()).unwrap();

    assert_eq!(
        rec.present(),
        vec!["apt:nftables".to_string()],
        "golem never installed the package, so reversal leaves it exactly as found"
    );
}

/// The second bug: both units notify `golem-nftables.service`, and the same
/// reconcile deletes its unit file and disables it. The reload set has to
/// subtract it rather than call `systemctl` on a unit that no longer exists
/// (ADR 0036, 2026-07-31 addendum).
#[test]
fn tearing_a_unit_down_never_pokes_the_unit_it_just_removed() {
    let room = Arc::new(MemoryPlanRoom::new());
    let rec = Ledger::new();
    let f = foreman(room.clone(), rec.clone());

    f.apply_manifest(&two_unit_manifest()).unwrap();
    rec.pokes.lock().unwrap().clear();

    let report = f.apply_manifest(&empty_manifest()).unwrap();

    assert_eq!(
        rec.pokes(),
        Vec::<String>::new(),
        "the unit whose unit file and service this reconcile removed is gone; \
         restarting or reloading it can only fail"
    );
    assert_eq!(
        report.outcome,
        TopOutcome::Settled,
        "a teardown that reversed every glyph is a settled apply, not a partial one"
    );
    assert!(
        report
            .units
            .iter()
            .all(|unit| unit.failures.is_empty() && unit.outcome == UnitOutcome::Settled),
        "no unit — the synthetic <reloads> group included — reports a failure"
    );
}

/// What an operator reads back afterwards. `GET /revisions` folds the same WAL
/// rows, so a receipt the fold got wrong would show up here as a revision
/// claiming state the host does not have.
#[test]
fn the_removal_revision_projects_the_applied_set_it_left_behind() {
    let room = Arc::new(MemoryPlanRoom::new());
    let rec = Ledger::new();
    let f = foreman(room.clone(), rec.clone());

    f.apply_manifest(&two_unit_manifest()).unwrap();
    f.apply_manifest(&empty_manifest()).unwrap();

    let revisions = f.revisions().unwrap();
    assert_eq!(revisions.len(), 3, "Init plus two committed reconciles");
    assert_eq!(
        revisions[1].outcomes.len(),
        8,
        "revision 2 is the applied set the first manifest left standing"
    );
    assert!(
        revisions[2].outcomes.is_empty(),
        "revision 3 is the applied set after removal — empty scroll, empty set"
    );
}
