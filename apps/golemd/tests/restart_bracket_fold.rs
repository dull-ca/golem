use std::sync::{Arc, Mutex};

use golemd::config::{OnExhaustConfig, RetryConfig};
use golemd::foreman::Foreman;
use golemd::journal::{AttemptPhase, GlyphOp, Inverse, Outcome, WalAction, WalStepState};
use golemd::planroom::{MemoryPlanRoom, PlanRoom};
use golemd::reconciler::{inverse_of, EnactError, EnactResult, Reconciler};
use golemd::report::{GlyphOutcome, UnitOutcome};
use scroll_format::{ContentId, Contents, Entry, Glyph, Manifest, Perms, Policy, Scroll};

/// A host model that applies files and services, remembers what is present so a
/// re-apply reads back its real inverse, records every restart, and captures the
/// inverse each `reverse` receives. A service key can be scripted to fail every
/// apply so a `keep` unit is left with a failed service while its config file
/// applied.
#[derive(Default)]
struct RestartHost {
    files: Mutex<std::collections::BTreeMap<String, String>>,
    services: Mutex<std::collections::BTreeMap<String, ContentId>>,
    restarts: Mutex<Vec<String>>,
    reversed_inverses: Mutex<Vec<(String, Inverse)>>,
    fail_service: Mutex<Vec<String>>,
}

impl RestartHost {
    fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
    fn fail_service_always(self: Arc<Self>, key: &str) -> Arc<Self> {
        self.fail_service.lock().unwrap().push(key.into());
        self
    }
    fn restarts(&self) -> Vec<String> {
        self.restarts.lock().unwrap().clone()
    }
    fn inverse_for(&self, key: &str) -> Option<Inverse> {
        self.reversed_inverses
            .lock()
            .unwrap()
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, i)| i.clone())
    }
}

impl Reconciler for RestartHost {
    fn apply(&self, glyph: &Glyph, cid: ContentId) -> EnactResult<Outcome> {
        let key = glyph.key();
        if self.fail_service.lock().unwrap().iter().any(|k| k == &key) {
            return Err(EnactError::Retryable(format!("scripted fail for {key}")));
        }
        match glyph {
            Glyph::Filesystem {
                path,
                entry: Entry::File { contents, perms },
            } => {
                let prior = self.files.lock().unwrap().get(path).cloned();
                if prior.as_deref() == Some(contents.as_str()) {
                    return Ok(Outcome {
                        op: GlyphOp::Install {
                            cid,
                            glyph: glyph.clone(),
                        },
                        cid,
                        inverse: Inverse::Nothing,
                        changed: false,
                    });
                }
                self.files
                    .lock()
                    .unwrap()
                    .insert(path.clone(), contents.clone());
                let inverse = match prior {
                    Some(p) => Inverse::RestoreFile {
                        path: path.clone(),
                        contents: p,
                        perms: perms.clone(),
                    },
                    None => Inverse::DeleteFile { path: path.clone() },
                };
                Ok(Outcome {
                    op: GlyphOp::Install {
                        cid,
                        glyph: glyph.clone(),
                    },
                    cid,
                    inverse,
                    changed: true,
                })
            }
            Glyph::SystemdService { .. } => {
                let already = self.services.lock().unwrap().get(&key) == Some(&cid);
                self.services.lock().unwrap().insert(key.clone(), cid);
                Ok(Outcome {
                    op: GlyphOp::Install {
                        cid,
                        glyph: glyph.clone(),
                    },
                    cid,
                    inverse: if already {
                        Inverse::Nothing
                    } else {
                        inverse_of(glyph)
                    },
                    changed: !already,
                })
            }
            _ => Ok(Outcome {
                op: GlyphOp::Install {
                    cid,
                    glyph: glyph.clone(),
                },
                cid,
                inverse: inverse_of(glyph),
                changed: true,
            }),
        }
    }

    fn reverse(&self, outcome: &Outcome) -> EnactResult<()> {
        self.reversed_inverses
            .lock()
            .unwrap()
            .push((outcome.op.key(), outcome.inverse.clone()));
        self.services.lock().unwrap().remove(&outcome.op.key());
        self.files
            .lock()
            .unwrap()
            .remove(&file_path_of(&outcome.op));
        Ok(())
    }

    fn restart_unit(&self, unit: &str) -> EnactResult<()> {
        self.restarts.lock().unwrap().push(unit.to_string());
        Ok(())
    }
}

fn file_path_of(op: &GlyphOp) -> String {
    match op.glyph() {
        Glyph::Filesystem { path, .. } => path.clone(),
        _ => String::new(),
    }
}

fn service(unit: &str) -> Glyph {
    Glyph::SystemdService { unit: unit.into() }
}

fn service_file(path: &str, contents: &str) -> Glyph {
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

fn keep_leaf(name: &str, glyphs: Vec<Glyph>) -> Scroll {
    Scroll {
        name: name.into(),
        policy: Some(Policy {
            on_exhaust: Some(scroll_format::OnExhaust::Keep),
            ..Policy::default()
        }),
        contents: Contents::Glyphs(glyphs),
    }
}

fn leaf(name: &str, glyphs: Vec<Glyph>) -> Scroll {
    Scroll {
        name: name.into(),
        policy: None,
        contents: Contents::Glyphs(glyphs),
    }
}

fn manifest(scroll: Scroll) -> Vec<u8> {
    scroll_format::to_bytes(&Manifest::from_scrolls(vec![scroll], "test"))
}

fn foreman(rec: Arc<RestartHost>) -> Foreman {
    Foreman::new("h1".into(), Box::new(MemoryPlanRoom::new()), Box::new(rec)).with_retry_config(
        RetryConfig {
            max_attempts: 1,
            base_delay_ms: 0,
            on_exhaust: OnExhaustConfig::Rollback,
            ..Default::default()
        },
    )
}

/// A unit under `keep` whose service glyph fails every round while its config
/// file applies. The failed service's config-restart bracket must not register
/// the service as applied, so the SECOND reconcile of the same manifest must
/// plan an attempt for the failed service again — reported partial, not settled.
#[test]
fn a_failed_service_is_not_masked_by_its_config_restart_bracket() {
    let rec = RestartHost::new().fail_service_always("systemd:app.service");
    let f = foreman(rec.clone());
    let scroll = Scroll {
        name: "h1".into(),
        policy: None,
        contents: Contents::Groups(vec![keep_leaf(
            "app",
            vec![
                service_file("/etc/systemd/system/app.service", "v1"),
                service("app.service"),
            ],
        )]),
    };
    let bytes = manifest(scroll);

    let first = f.apply_manifest(&bytes).unwrap();
    assert_eq!(first.units[0].outcome, UnitOutcome::Partial);
    assert!(first.units[0]
        .failures
        .iter()
        .any(|x| x.glyph_key == "systemd:app.service"));

    let second = f.apply_manifest(&bytes).unwrap();
    let unit = &second.units[0];
    assert_eq!(
        unit.outcome,
        UnitOutcome::Partial,
        "the second reconcile must re-attempt the failed service, not report it settled"
    );
    assert!(
        unit.failures
            .iter()
            .any(|x| x.glyph_key == "systemd:app.service"),
        "the failed service must show as failed again, not masked to Noop"
    );
    let service_line = unit
        .glyphs
        .iter()
        .find(|g| g.glyph_key == "systemd:app.service")
        .unwrap();
    assert_eq!(
        service_line.outcome,
        GlyphOutcome::Failed,
        "the service must diff as an attempt (Failed), never Unchanged"
    );
}

/// A healthy service and its config file both apply; a config-only change fires a
/// propagate restart; then a manifest dropping the service must reverse it with
/// the REAL recorded inverse (`DisableSystemdService`), not the restart bracket's
/// `Inverse::Nothing`.
#[test]
fn dropping_a_restarted_service_reverses_with_the_real_inverse() {
    let rec = RestartHost::new();
    let f = foreman(rec.clone());

    f.apply_manifest(&manifest(leaf(
        "h1",
        vec![
            service_file("/etc/systemd/system/app.service", "v1"),
            service("app.service"),
        ],
    )))
    .unwrap();

    rec.restarts.lock().unwrap().clear();
    f.apply_manifest(&manifest(leaf(
        "h1",
        vec![
            service_file("/etc/systemd/system/app.service", "v2"),
            service("app.service"),
        ],
    )))
    .unwrap();
    assert_eq!(
        rec.restarts(),
        vec!["app.service".to_string()],
        "the config-only change restarts the mapped service"
    );

    f.apply_manifest(&manifest(leaf(
        "h1",
        vec![service_file("/etc/systemd/system/app.service", "v2")],
    )))
    .unwrap();

    let inverse = rec
        .inverse_for("systemd:app.service")
        .expect("the dropped service is reversed");
    assert!(
        matches!(inverse, Inverse::DisableSystemdService { .. }),
        "reverse must receive the service's real recorded inverse, not the restart bracket's Nothing: got {inverse:?}"
    );
}

/// An attempt interrupted after a `Restart` Intended row recovers cleanly: the
/// restart is a non-reversible operational record, so recovery re-runs the
/// idempotent try-restart (never reverses it) and rolls the unsettled attempt
/// back to its last committed set.
#[test]
fn a_crash_after_a_restart_intended_recovers_cleanly() {
    let room = Arc::new(MemoryPlanRoom::new());
    let glyph = service("app.service");
    let cid = scroll_format::content_id_of_glyph(&glyph);
    let apply_op = GlyphOp::Install {
        cid,
        glyph: glyph.clone(),
    };
    let restart_op = GlyphOp::Noop {
        cid,
        glyph: glyph.clone(),
    };

    room.open_attempt(Some(cid)).unwrap();
    room.set_attempt_phase(1, AttemptPhase::Enacting).unwrap();
    room.append_wal_step(
        1,
        0,
        "systemd:app.service",
        WalAction::Apply,
        WalStepState::Intended,
        &apply_op,
        Some(&inverse_of(&glyph)),
        None,
        &["h1".into()],
    )
    .unwrap();
    room.append_wal_step(
        1,
        0,
        "systemd:app.service",
        WalAction::Apply,
        WalStepState::Done,
        &apply_op,
        Some(&inverse_of(&glyph)),
        Some(true),
        &["h1".into()],
    )
    .unwrap();
    room.append_wal_step(
        1,
        1,
        "restart:app.service",
        WalAction::Restart,
        WalStepState::Intended,
        &restart_op,
        Some(&Inverse::Nothing),
        None,
        &["h1".into()],
    )
    .unwrap();

    let rec = RestartHost::new();
    let _f = Foreman::new("h1".into(), Box::new(room.clone()), Box::new(rec.clone()))
        .with_retry_config(RetryConfig {
            max_attempts: 1,
            base_delay_ms: 0,
            ..Default::default()
        });

    let settled = room.latest_attempt().unwrap().unwrap();
    assert_eq!(
        settled.phase,
        AttemptPhase::RolledBack,
        "recovery settles the crashed attempt"
    );
    assert_eq!(
        rec.restarts(),
        vec!["app.service".to_string()],
        "the interrupted restart's try-restart is re-run idempotently, not reversed"
    );
    let inverses = rec.reversed_inverses.lock().unwrap();
    assert!(
        inverses.iter().all(|(k, _)| k != "restart:app.service"),
        "a Restart step is never reversed"
    );
    assert!(
        inverses.iter().any(|(k, _)| k == "systemd:app.service"),
        "the unsettled attempt's applied service is rolled back"
    );
}
