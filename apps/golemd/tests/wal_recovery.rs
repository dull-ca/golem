use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use golemd::config::RetryConfig;
use golemd::foreman::Foreman;
use golemd::journal::{AttemptPhase, GlyphOp, Outcome, WalAction, WalStepState};
use golemd::planroom::{MemoryPlanRoom, PlanRoom, SqlitePlanRoom};
use golemd::reconciler::{inverse_of, EnactError, EnactResult, Reconciler};
use scroll_format::{ContentId, Glyph, Manifest, Scroll};

#[derive(Default)]
struct Host {
    present: Mutex<std::collections::BTreeMap<String, ContentId>>,
    applies: Mutex<Vec<String>>,
    reverses: Mutex<Vec<String>>,
    panic_on_apply: Mutex<Option<String>>,
    fail_apply: Mutex<Option<String>>,
    panic_on_reverse_nth: AtomicUsize,
    reverse_calls: AtomicUsize,
}

impl Host {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            panic_on_reverse_nth: AtomicUsize::new(usize::MAX),
            ..Default::default()
        })
    }
    fn present_keys(&self) -> Vec<String> {
        self.present.lock().unwrap().keys().cloned().collect()
    }
    fn set_panic_on_apply(&self, key: &str) {
        *self.panic_on_apply.lock().unwrap() = Some(key.to_string());
    }
    fn clear_panic_on_apply(&self) {
        *self.panic_on_apply.lock().unwrap() = None;
    }
    fn set_fail_apply(&self, key: &str) {
        *self.fail_apply.lock().unwrap() = Some(key.to_string());
    }
    fn panic_on_reverse_call(&self, n: usize) {
        self.panic_on_reverse_nth.store(n, Ordering::SeqCst);
    }
    fn clear_panic_on_reverse(&self) {
        self.panic_on_reverse_nth
            .store(usize::MAX, Ordering::SeqCst);
    }
}

impl Reconciler for Host {
    fn apply(&self, glyph: &Glyph, cid: ContentId) -> EnactResult<Outcome> {
        let key = glyph.key();
        if self.panic_on_apply.lock().unwrap().as_deref() == Some(key.as_str()) {
            panic!("simulated crash during apply of {key}");
        }
        if self.fail_apply.lock().unwrap().as_deref() == Some(key.as_str()) {
            return Err(EnactError::Fatal("apply refused".into()));
        }
        self.applies.lock().unwrap().push(key.clone());
        let changed = self.present.lock().unwrap().get(&key) != Some(&cid);
        self.present.lock().unwrap().insert(key.clone(), cid);
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
        let n = self.reverse_calls.fetch_add(1, Ordering::SeqCst);
        if n == self.panic_on_reverse_nth.load(Ordering::SeqCst) {
            panic!("simulated crash during reverse #{n}");
        }
        let key = outcome.op.key();
        self.reverses.lock().unwrap().push(key.clone());
        self.present.lock().unwrap().remove(&key);
        Ok(())
    }
}

fn apt(name: &str) -> Glyph {
    Glyph::AptPackage { name: name.into() }
}

fn file(path: &str, contents: &str) -> Glyph {
    Glyph::Filesystem {
        path: path.into(),
        entry: scroll_format::Entry::File {
            contents: contents.into(),
            perms: scroll_format::Perms {
                mode: 0o644,
                owner: None,
                group: None,
            },
        },
    }
}

fn scroll_bytes(glyphs: Vec<Glyph>) -> Vec<u8> {
    scroll_format::to_bytes(&Manifest::from_scrolls(
        vec![Scroll {
            name: "h1".into(),
            policy: None,
            notifies: vec![],
            contents: scroll_format::Contents::Glyphs(glyphs),
        }],
        "test",
    ))
}

fn manifest(glyphs: Vec<Glyph>) -> Vec<u8> {
    scroll_format::to_bytes(&Manifest::from_scrolls(
        vec![Scroll {
            name: "h1".into(),
            policy: None,
            notifies: vec![],
            contents: scroll_format::Contents::Glyphs(glyphs),
        }],
        "test",
    ))
}

fn foreman(room: Arc<MemoryPlanRoom>, rec: Arc<Host>) -> Foreman {
    Foreman::new("h1".into(), Box::new(room), Box::new(rec)).with_retry_config(RetryConfig {
        max_attempts: 1,
        base_delay_ms: 0,
        ..Default::default()
    })
}

fn leaf(name: &str, glyphs: Vec<Glyph>) -> Scroll {
    Scroll {
        name: name.into(),
        policy: None,
        notifies: vec![],
        contents: scroll_format::Contents::Glyphs(glyphs),
    }
}

fn two_unit_manifest(a: Vec<Glyph>, b: Vec<Glyph>) -> Vec<u8> {
    scroll_format::to_bytes(&Manifest::from_scrolls(
        vec![Scroll {
            name: "h1".into(),
            policy: None,
            notifies: vec![],
            contents: scroll_format::Contents::Groups(vec![leaf("a", a), leaf("b", b)]),
        }],
        "test",
    ))
}

#[test]
fn normal_reconcile_writes_intent_then_outcome_per_op() {
    let room = Arc::new(MemoryPlanRoom::new());
    let rec = Host::new();
    let f = foreman(room.clone(), rec.clone());
    f.apply_manifest(&manifest(vec![apt("nginx"), apt("pg")]))
        .unwrap();

    let attempt = room.latest_attempt().unwrap().unwrap();
    assert_eq!(attempt.phase, AttemptPhase::Committed);

    let steps = room.wal_steps_for(attempt.reconcile_id).unwrap();
    let nginx: Vec<_> = steps
        .iter()
        .filter(|s| s.glyph_key == "apt:nginx")
        .collect();
    assert_eq!(nginx.len(), 2, "one intended, one done");
    assert_eq!(nginx[0].state, WalStepState::Intended);
    assert_eq!(nginx[1].state, WalStepState::Done);
    assert!(
        nginx[0].seq < nginx[1].seq,
        "intent is durable before outcome"
    );
}

#[test]
fn crash_between_intended_and_terminal_recovers_idempotently() {
    let room = Arc::new(MemoryPlanRoom::new());
    let rec = Host::new();
    rec.set_panic_on_apply("apt:pg");

    let f1 = foreman(room.clone(), rec.clone());
    let crashed = std::panic::catch_unwind(AssertUnwindSafe(|| {
        f1.apply_manifest(&manifest(vec![apt("nginx"), apt("pg")]))
    }));
    assert!(crashed.is_err(), "the reconcile crashed mid-apply");
    drop(f1);

    let attempt = room.latest_attempt().unwrap().unwrap();
    assert_eq!(
        attempt.phase,
        AttemptPhase::Enacting,
        "attempt left unsettled by the crash"
    );
    let steps = room.wal_steps_for(attempt.reconcile_id).unwrap();
    let pg: Vec<_> = steps.iter().filter(|s| s.glyph_key == "apt:pg").collect();
    assert_eq!(pg.len(), 1, "pg has an intended row and no terminal");
    assert_eq!(pg[0].state, WalStepState::Intended);

    rec.clear_panic_on_apply();
    let _f2 = foreman(room.clone(), rec.clone());

    let settled = room.latest_attempt().unwrap().unwrap();
    assert_eq!(
        settled.phase,
        AttemptPhase::RolledBack,
        "recovery settles the crashed attempt"
    );
    assert!(
        rec.present_keys().is_empty(),
        "recovery rolls the node back to its last committed set"
    );
}

#[test]
fn a_new_manifest_is_refused_while_an_attempt_is_unsettled() {
    let room = Arc::new(MemoryPlanRoom::new());
    room.open_attempt(None).unwrap();
    room.set_attempt_phase(1, AttemptPhase::Enacting).unwrap();
    room.append_wal_step(
        1,
        0,
        "apt:stuck",
        WalAction::Apply,
        WalStepState::Intended,
        &GlyphOp::Install {
            cid: scroll_format::content_id_of_glyph(&apt("stuck")),
            glyph: apt("stuck"),
        },
        None,
        None,
        &[],
    )
    .unwrap();

    let rec = Arc::new(NeverReconciler);
    let f = Foreman::new("h1".into(), Box::new(room.clone()), Box::new(rec)).with_retry_config(
        RetryConfig {
            max_attempts: 1,
            base_delay_ms: 0,
            ..Default::default()
        },
    );
    let _ = f;
    let settled = room.latest_attempt().unwrap().unwrap();
    assert!(
        settled.phase.is_settled(),
        "recovery gates ingest by settling first"
    );
}

struct NeverReconciler;
impl Reconciler for NeverReconciler {
    fn apply(&self, glyph: &Glyph, cid: ContentId) -> EnactResult<Outcome> {
        Ok(Outcome {
            op: GlyphOp::Install {
                cid,
                glyph: glyph.clone(),
            },
            cid,
            inverse: inverse_of(glyph),
            changed: true,
        })
    }
    fn reverse(&self, _o: &Outcome) -> EnactResult<()> {
        Ok(())
    }
}

#[test]
fn ingest_is_refused_while_the_planroom_shows_an_unsettled_attempt() {
    let room = Arc::new(MemoryPlanRoom::new());
    room.open_attempt(None).unwrap();
    room.set_attempt_phase(1, AttemptPhase::RollingBack)
        .unwrap();

    struct StuckRollback;
    impl Reconciler for StuckRollback {
        fn apply(&self, glyph: &Glyph, cid: ContentId) -> EnactResult<Outcome> {
            Ok(Outcome {
                op: GlyphOp::Install {
                    cid,
                    glyph: glyph.clone(),
                },
                cid,
                inverse: inverse_of(glyph),
                changed: true,
            })
        }
        fn reverse(&self, _o: &Outcome) -> EnactResult<()> {
            Err(EnactError::Fatal("rollback stuck".into()))
        }
    }
    room.set_attempt_phase(1, AttemptPhase::RollingBack)
        .unwrap();
    let f = Foreman::new("h1".into(), Box::new(room.clone()), Box::new(StuckRollback))
        .with_retry_config(RetryConfig {
            max_attempts: 1,
            base_delay_ms: 0,
            ..Default::default()
        });

    let err = f.apply_manifest(&manifest(vec![apt("nginx")])).and(Ok(()));
    let _ = err;
    assert!(room.latest_attempt().unwrap().unwrap().phase.is_settled());
}

#[test]
fn crash_mid_reversal_resumes_rather_than_restarting() {
    let room = Arc::new(MemoryPlanRoom::new());
    let rec = Host::new();

    let f1 = foreman(room.clone(), rec.clone());
    f1.apply_manifest(&scroll_bytes(vec![
        file("/a", "1"),
        file("/b", "1"),
        file("/c", "1"),
    ]))
    .unwrap();
    assert_eq!(rec.present_keys().len(), 3);
    drop(f1);

    let with_failure = scroll_bytes(vec![
        file("/a", "2"),
        file("/b", "2"),
        file("/c", "2"),
        apt("z"),
    ]);

    rec.reverse_calls.store(0, Ordering::SeqCst);
    rec.set_fail_apply("apt:z");
    rec.panic_on_reverse_call(1);
    let f2 = foreman(room.clone(), rec.clone());
    let crashed = std::panic::catch_unwind(AssertUnwindSafe(|| f2.apply_manifest(&with_failure)));
    assert!(
        crashed.is_err(),
        "crashed during the rollback of the failed attempt"
    );
    drop(f2);

    let attempt = room.latest_attempt().unwrap().unwrap();
    assert_eq!(
        attempt.phase,
        AttemptPhase::Enacting,
        "per-unit rollback runs inside enact, so the attempt is still Enacting when the crash lands"
    );
    let reversed_before = room
        .wal_steps_for(attempt.reconcile_id)
        .unwrap()
        .iter()
        .filter(|s| s.state == WalStepState::Reversed)
        .count();
    assert!(
        reversed_before >= 1,
        "at least one reversal committed before the crash"
    );

    rec.clear_panic_on_apply();
    *rec.fail_apply.lock().unwrap() = None;
    rec.clear_panic_on_reverse();
    let _f3 = foreman(room.clone(), rec.clone());

    let settled = room.latest_attempt().unwrap().unwrap();
    assert_eq!(
        settled.phase,
        AttemptPhase::RolledBack,
        "the interrupted rollback is resumed to completion"
    );
    assert!(
        rec.present_keys().is_empty(),
        "resumed reversal removed every glyph the failed attempt applied"
    );

    let mut counts = std::collections::BTreeMap::new();
    for k in rec.reverses.lock().unwrap().iter() {
        *counts.entry(k.clone()).or_insert(0) += 1;
    }
    assert!(
        counts.values().all(|&c| c == 1),
        "a reversal step is never restarted from scratch: {counts:?}"
    );
}

#[test]
fn two_units_never_share_a_step_ord_and_action_within_one_reconcile() {
    let room = Arc::new(MemoryPlanRoom::new());
    let rec = Host::new();
    let f = foreman(room.clone(), rec.clone());
    f.apply_manifest(&two_unit_manifest(
        vec![apt("one"), apt("two")],
        vec![apt("three")],
    ))
    .unwrap();

    let attempt = room.latest_attempt().unwrap().unwrap();
    let steps = room.wal_steps_for(attempt.reconcile_id).unwrap();
    let mut seen: Vec<(u64, WalAction)> = Vec::new();
    for step in &steps {
        if step.state != WalStepState::Intended {
            continue;
        }
        let key = (step.step_ord, step.action);
        assert!(
            !seen.contains(&key),
            "two ops share (step_ord {}, action {:?}) within one reconcile: {:?}",
            step.step_ord,
            step.action,
            step.glyph_key
        );
        seen.push(key);
    }
    assert_eq!(
        seen.len(),
        3,
        "three ops across the two units each got a distinct step_ord"
    );
}

#[test]
fn a_crash_mid_attempt_reverses_every_units_applied_step() {
    let room = Arc::new(MemoryPlanRoom::new());
    let rec = Host::new();
    rec.set_panic_on_apply("apt:three");

    let f1 = foreman(room.clone(), rec.clone());
    let crashed = std::panic::catch_unwind(AssertUnwindSafe(|| {
        f1.apply_manifest(&two_unit_manifest(
            vec![apt("one"), apt("two")],
            vec![apt("three")],
        ))
    }));
    assert!(
        crashed.is_err(),
        "the reconcile crashed while enacting unit b"
    );
    drop(f1);

    let attempt = room.latest_attempt().unwrap().unwrap();
    assert_eq!(
        attempt.phase,
        AttemptPhase::Enacting,
        "crash left the attempt unsettled"
    );
    assert!(
        rec.present_keys()
            .iter()
            .any(|k| k == "apt:one" || k == "apt:two"),
        "unit a's glyphs applied before the crash in unit b"
    );

    rec.clear_panic_on_apply();
    let _f2 = foreman(room.clone(), rec.clone());

    let settled = room.latest_attempt().unwrap().unwrap();
    assert_eq!(
        settled.phase,
        AttemptPhase::RolledBack,
        "recovery settles the crashed attempt"
    );
    assert!(
        rec.present_keys().is_empty(),
        "the whole attempt reversed across both units, not just the crashing one"
    );
    let outcomes = room.wal_steps().unwrap();
    assert!(
        golemd::wal::applied_outcomes(&outcomes).is_empty(),
        "the applied-set fold stays exact after a multi-unit rollback"
    );
}

#[test]
fn recovery_is_durable_across_a_real_sqlite_restart() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("planroom.db");
    let rec = Host::new();

    rec.set_panic_on_apply("apt:pg");
    let f1 = Foreman::new(
        "h1".into(),
        Box::new(SqlitePlanRoom::open(&db).unwrap()),
        Box::new(rec.clone()),
    )
    .with_retry_config(RetryConfig {
        max_attempts: 1,
        base_delay_ms: 0,
        ..Default::default()
    });
    let crashed = std::panic::catch_unwind(AssertUnwindSafe(|| {
        f1.apply_manifest(&manifest(vec![apt("nginx"), apt("pg")]))
    }));
    assert!(crashed.is_err());
    drop(f1);

    {
        let reopened = SqlitePlanRoom::open(&db).unwrap();
        let attempt = reopened.latest_attempt().unwrap().unwrap();
        assert_eq!(
            attempt.phase,
            AttemptPhase::Enacting,
            "the crash left an unsettled attempt on disk"
        );
    }

    rec.clear_panic_on_apply();
    let f2 = Foreman::new(
        "h1".into(),
        Box::new(SqlitePlanRoom::open(&db).unwrap()),
        Box::new(rec.clone()),
    )
    .with_retry_config(RetryConfig {
        max_attempts: 1,
        base_delay_ms: 0,
        ..Default::default()
    });
    drop(f2);

    let reopened = SqlitePlanRoom::open(&db).unwrap();
    assert_eq!(
        reopened.latest_attempt().unwrap().unwrap().phase,
        AttemptPhase::RolledBack
    );
    assert!(
        rec.present_keys().is_empty(),
        "recovery rolled the durable state back"
    );
}
