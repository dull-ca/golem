use std::sync::{Arc, Mutex};
use std::time::Duration;

use golemd::foreman::Foreman;
use golemd::journal::{AttemptPhase, GlyphOp, Outcome, RevisionKind, WalAction, WalStepState};
use golemd::planroom::{MemoryPlanRoom, PlanRoom, SqlitePlanRoom};
use golemd::reconciler::{inverse_of, EnactResult, Reconciler};
use scroll_format::{ContentId, Glyph, Manifest, Scroll};

#[derive(Default)]
struct Host {
    present: Mutex<std::collections::BTreeMap<String, ContentId>>,
}
impl Reconciler for Host {
    fn apply(&self, glyph: &Glyph, cid: ContentId) -> EnactResult<Outcome> {
        let key = glyph.key();
        let changed = self.present.lock().unwrap().get(&key) != Some(&cid);
        self.present.lock().unwrap().insert(key.clone(), cid);
        Ok(Outcome {
            op: GlyphOp::Install { cid, glyph: glyph.clone() },
            cid,
            inverse: inverse_of(glyph),
            changed,
        })
    }
    fn reverse(&self, outcome: &Outcome) -> EnactResult<()> {
        self.present.lock().unwrap().remove(&outcome.op.key());
        Ok(())
    }
}

fn apt(name: &str) -> Glyph {
    Glyph::AptPackage { name: name.into() }
}

fn manifest(glyphs: Vec<Glyph>) -> Vec<u8> {
    scroll_format::to_bytes(&Manifest::from_scrolls(vec![Scroll { name: "h1".into(), glyphs }], "test"))
}

fn foreman(room: Arc<MemoryPlanRoom>) -> Foreman {
    Foreman::new("h1".into(), Box::new(room), Box::new(Host::default())).with_retry(1, Duration::ZERO)
}

#[test]
fn a_committed_attempt_yields_exactly_one_reconcile_revision() {
    let room = Arc::new(MemoryPlanRoom::new());
    let f = foreman(room.clone());
    f.apply_manifest(&manifest(vec![apt("nginx")])).unwrap();

    let revisions = f.revisions().unwrap();
    assert_eq!(revisions.len(), 2, "Init plus one Reconcile");
    assert_eq!(revisions[0].kind, RevisionKind::Init);
    assert_eq!(revisions[1].kind, RevisionKind::Reconcile);

    let committed: Vec<_> = room
        .attempts()
        .unwrap()
        .into_iter()
        .filter(|a| a.phase == AttemptPhase::Committed)
        .collect();
    assert_eq!(committed.len(), 1);
    let reconcile_revisions = revisions.iter().filter(|r| r.kind == RevisionKind::Reconcile).count();
    assert_eq!(reconcile_revisions, committed.len(), "one revision per committed attempt");
}

#[test]
fn the_revision_projects_the_attempts_applied_ops() {
    let room = Arc::new(MemoryPlanRoom::new());
    let f = foreman(room.clone());
    f.apply_manifest(&manifest(vec![apt("nginx"), apt("pg")])).unwrap();

    let revisions = f.revisions().unwrap();
    let latest = revisions.last().unwrap();
    let keys: Vec<String> = latest.outcomes.iter().map(|o| o.op.key()).collect();
    assert!(keys.contains(&"apt:nginx".to_string()));
    assert!(keys.contains(&"apt:pg".to_string()));
    assert_eq!(latest.scroll_content_id.is_some(), true);
}

#[test]
fn a_rolled_back_attempt_projects_no_revision() {
    let room = Arc::new(MemoryPlanRoom::new());
    room.open_attempt(None).unwrap();
    room.set_attempt_phase(1, AttemptPhase::RolledBack).unwrap();

    let f = foreman(room.clone());
    let revisions = f.revisions().unwrap();
    assert_eq!(revisions.len(), 1, "only Init; a rolled-back attempt is not history");
    assert_eq!(revisions[0].kind, RevisionKind::Init);
}

#[test]
fn revision_by_id_matches_the_list() {
    let room = Arc::new(MemoryPlanRoom::new());
    let f = foreman(room.clone());
    f.apply_manifest(&manifest(vec![apt("nginx")])).unwrap();
    f.apply_manifest(&manifest(vec![apt("nginx"), apt("pg")])).unwrap();

    let revisions = f.revisions().unwrap();
    for rev in &revisions {
        assert_eq!(f.revision(rev.id).unwrap().as_ref(), Some(rev));
    }
    assert_eq!(f.latest_revision_id().unwrap(), Some(revisions.last().unwrap().id));
    assert!(f.revision(9_999).unwrap().is_none());
}

#[test]
fn a_committed_attempt_with_no_separate_revision_row_still_projects() {
    let room = Arc::new(MemoryPlanRoom::new());
    room.open_attempt(None).unwrap();
    let op = GlyphOp::Install {
        cid: scroll_format::content_id_of_glyph(&apt("nginx")),
        glyph: apt("nginx"),
    };
    room.append_wal_step(1, 0, "apt:nginx", WalAction::Apply, WalStepState::Intended, &op, None, None)
        .unwrap();
    room.append_wal_step(
        1,
        0,
        "apt:nginx",
        WalAction::Apply,
        WalStepState::Done,
        &op,
        Some(&inverse_of(&apt("nginx"))),
        Some(true),
    )
    .unwrap();
    room.set_attempt_phase(1, AttemptPhase::Committed).unwrap();

    let f = foreman(room.clone());
    let revisions = f.revisions().unwrap();
    assert_eq!(revisions.len(), 2, "the committed attempt is projected with no separately-appended row");
    assert_eq!(revisions[1].kind, RevisionKind::Reconcile);
    let keys: Vec<String> = revisions[1].outcomes.iter().map(|o| o.op.key()).collect();
    assert_eq!(keys, vec!["apt:nginx".to_string()]);
}

#[test]
fn sqlite_and_memory_project_the_same_revisions() {
    fn run(room: &dyn PlanRoom) -> (usize, Vec<RevisionKind>) {
        room.open_attempt(None).unwrap();
        let op = GlyphOp::Install {
            cid: scroll_format::content_id_of_glyph(&apt("nginx")),
            glyph: apt("nginx"),
        };
        room.append_wal_step(1, 0, "apt:nginx", WalAction::Apply, WalStepState::Done, &op, None, Some(true))
            .unwrap();
        room.set_attempt_phase(1, AttemptPhase::Committed).unwrap();
        let revs = room.revisions().unwrap();
        (revs.len(), revs.iter().map(|r| r.kind).collect())
    }
    assert_eq!(
        run(&MemoryPlanRoom::new()),
        run(&SqlitePlanRoom::open(std::path::Path::new(":memory:")).unwrap())
    );
}
