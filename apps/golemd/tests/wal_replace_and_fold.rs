use std::sync::{Arc, Mutex};
use std::time::Duration;

use golemd::foreman::Foreman;
use golemd::journal::{GlyphOp, Inverse, Outcome, WalAction, WalStepState};
use golemd::planroom::{MemoryPlanRoom, PlanRoom};
use golemd::reconciler::{EnactError, EnactResult, Reconciler};
use golemd::wal::applied_outcomes;
use scroll_format::{ContentId, Entry, Glyph, Manifest, Perms, Scroll};

#[derive(Default)]
struct FileHost {
    files: Mutex<std::collections::BTreeMap<String, String>>,
}

impl FileHost {
    fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
    fn contents(&self, path: &str) -> Option<String> {
        self.files.lock().unwrap().get(path).cloned()
    }
}

impl Reconciler for FileHost {
    fn apply(&self, glyph: &Glyph, cid: ContentId) -> EnactResult<Outcome> {
        let (path, contents, perms) = match glyph {
            Glyph::Filesystem { path, entry: Entry::File { contents, perms } } => (path, contents, perms),
            _ => return Err(EnactError::Fatal("only files here".into())),
        };
        let prior = self.files.lock().unwrap().get(path).cloned();
        if prior.as_deref() == Some(contents.as_str()) {
            return Ok(Outcome {
                op: GlyphOp::Install { cid, glyph: glyph.clone() },
                cid,
                inverse: Inverse::Nothing,
                changed: false,
            });
        }
        self.files.lock().unwrap().insert(path.clone(), contents.clone());
        let inverse = match prior {
            Some(prior_contents) => Inverse::RestoreFile {
                path: path.clone(),
                contents: prior_contents,
                perms: perms.clone(),
            },
            None => Inverse::DeleteFile { path: path.clone() },
        };
        Ok(Outcome { op: GlyphOp::Install { cid, glyph: glyph.clone() }, cid, inverse, changed: true })
    }
    fn reverse(&self, outcome: &Outcome) -> EnactResult<()> {
        match &outcome.inverse {
            Inverse::Nothing => {}
            Inverse::RestoreFile { path, contents, .. } => {
                self.files.lock().unwrap().insert(path.clone(), contents.clone());
            }
            Inverse::DeleteFile { path } => {
                self.files.lock().unwrap().remove(path);
            }
            other => return Err(EnactError::Fatal(format!("unexpected inverse {other:?}"))),
        }
        Ok(())
    }
}

fn file(path: &str, contents: &str) -> Glyph {
    Glyph::Filesystem {
        path: path.into(),
        entry: Entry::File { contents: contents.into(), perms: Perms { mode: 0o644, owner: None, group: None } },
    }
}

fn manifest(glyphs: Vec<Glyph>) -> Vec<u8> {
    scroll_format::to_bytes(&Manifest::from_scrolls(vec![Scroll { name: "h1".into(), glyphs }], "test"))
}

fn foreman(room: Arc<MemoryPlanRoom>, rec: Arc<FileHost>) -> Foreman {
    Foreman::new("h1".into(), Box::new(room), Box::new(rec)).with_retry(1, Duration::ZERO)
}

#[test]
fn in_place_file_replace_is_one_apply_step_capturing_the_prior_bytes() {
    let room = Arc::new(MemoryPlanRoom::new());
    let rec = FileHost::new();
    let f = foreman(room.clone(), rec.clone());

    f.apply_manifest(&manifest(vec![file("/etc/app.conf", "v1")])).unwrap();
    f.apply_manifest(&manifest(vec![file("/etc/app.conf", "v2")])).unwrap();

    assert_eq!(rec.contents("/etc/app.conf").as_deref(), Some("v2"));

    let attempt = room.latest_attempt().unwrap().unwrap();
    let steps = room.wal_steps_for(attempt.reconcile_id).unwrap();
    let for_file: Vec<_> = steps.iter().filter(|s| s.glyph_key.starts_with("file:")).collect();
    assert_eq!(for_file.len(), 2, "a file Replace is one apply step: intended + done, no separate reverse");
    assert!(for_file.iter().all(|s| s.action == WalAction::Apply));
    let done = for_file.iter().find(|s| s.state == WalStepState::Done).unwrap();
    assert_eq!(
        done.inverse,
        Some(Inverse::RestoreFile { path: "/etc/app.conf".into(), contents: "v1".into(), perms: Perms { mode: 0o644, owner: None, group: None } }),
        "the single apply records the pre-Replace bytes as its inverse"
    );
}

#[test]
fn reversing_an_in_place_file_replace_restores_v1_exactly() {
    let room = Arc::new(MemoryPlanRoom::new());
    let rec = FileHost::new();
    let f = foreman(room.clone(), rec.clone());

    f.apply_manifest(&manifest(vec![file("/etc/app.conf", "v1")])).unwrap();
    f.apply_manifest(&manifest(vec![file("/etc/app.conf", "v2")])).unwrap();
    f.apply_manifest(&manifest(vec![])).unwrap();

    assert_eq!(
        rec.contents("/etc/app.conf").as_deref(),
        Some("v1"),
        "reversing the Replace restores the whole pre-Replace file, not deletes it"
    );
}

#[test]
fn the_applied_fold_matches_the_sequence_of_applies_and_reverses() {
    let room = Arc::new(MemoryPlanRoom::new());
    let rec = FileHost::new();
    let f = foreman(room.clone(), rec.clone());

    f.apply_manifest(&manifest(vec![file("/a", "1"), file("/b", "1")])).unwrap();
    f.apply_manifest(&manifest(vec![file("/a", "2")])).unwrap();

    let fold = applied_outcomes(&room.wal_steps().unwrap());
    let keys: Vec<_> = fold.iter().map(|o| o.op.key()).collect();
    assert_eq!(keys, vec!["file:/a".to_string()], "b was removed, a survives at v2");

    let a = &fold[0];
    assert!(a.op.glyph_content_id_matches("2"), "the survivor is at v2, the applied version");
}

trait GlyphContentPeek {
    fn glyph_content_id_matches(&self, contents: &str) -> bool;
}
impl GlyphContentPeek for GlyphOp {
    fn glyph_content_id_matches(&self, contents: &str) -> bool {
        matches!(self.glyph(), Glyph::Filesystem { entry: Entry::File { contents: c, .. }, .. } if c == contents)
    }
}
