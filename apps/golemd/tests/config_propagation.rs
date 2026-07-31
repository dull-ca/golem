use std::sync::{Arc, Mutex};

use golemd::config::RetryConfig;
use golemd::foreman::Foreman;
use golemd::journal::{GlyphOp, Inverse, Outcome};
use golemd::planroom::MemoryPlanRoom;
use golemd::reconciler::{EnactResult, Reconciler};
use scroll_format::{ContentId, Entry, Glyph, Manifest, Perms, Scroll};

#[derive(Default)]
struct RestartRecorder {
    files: Mutex<std::collections::BTreeMap<String, String>>,
    restarts: Mutex<Vec<String>>,
}

impl RestartRecorder {
    fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
    fn restarts(&self) -> Vec<String> {
        self.restarts.lock().unwrap().clone()
    }
}

impl Reconciler for RestartRecorder {
    fn apply(&self, glyph: &Glyph, cid: ContentId) -> EnactResult<Outcome> {
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
            _ => Ok(Outcome {
                op: GlyphOp::Install {
                    cid,
                    glyph: glyph.clone(),
                },
                cid,
                inverse: Inverse::Nothing,
                changed: true,
            }),
        }
    }
    fn reverse(&self, _o: &Outcome) -> EnactResult<()> {
        Ok(())
    }
    fn restart_unit(&self, unit: &str) -> EnactResult<()> {
        self.restarts.lock().unwrap().push(unit.to_string());
        Ok(())
    }
}

fn quadlet(path: &str, contents: &str) -> Glyph {
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

fn foreman(rec: Arc<RestartRecorder>) -> Foreman {
    Foreman::new("h1".into(), Box::new(MemoryPlanRoom::new()), Box::new(rec)).with_retry_config(
        RetryConfig {
            max_attempts: 1,
            base_delay_ms: 0,
            ..Default::default()
        },
    )
}

#[test]
fn a_changed_quadlet_file_restarts_its_mapped_unit() {
    let rec = RestartRecorder::new();
    let f = foreman(rec.clone());

    f.apply_manifest(&manifest(vec![quadlet(
        "/etc/containers/systemd/registry.container",
        "v1",
    )]))
    .unwrap();
    assert_eq!(
        rec.restarts(),
        vec!["registry.service".to_string()],
        "a new quadlet file restarts registry.service"
    );

    rec.restarts.lock().unwrap().clear();
    f.apply_manifest(&manifest(vec![quadlet(
        "/etc/containers/systemd/registry.container",
        "v2",
    )]))
    .unwrap();
    assert_eq!(
        rec.restarts(),
        vec!["registry.service".to_string()],
        "a changed quadlet file restarts the mapped unit"
    );
}

#[test]
fn an_unchanged_quadlet_file_restarts_nothing() {
    let rec = RestartRecorder::new();
    let f = foreman(rec.clone());
    let bytes = manifest(vec![quadlet(
        "/etc/containers/systemd/registry.container",
        "v1",
    )]);
    f.apply_manifest(&bytes).unwrap();
    rec.restarts.lock().unwrap().clear();
    f.apply_manifest(&bytes).unwrap();
    assert!(rec.restarts().is_empty(), "a Noop file triggers no restart");
}

#[test]
fn a_file_outside_a_unit_directory_restarts_nothing() {
    let rec = RestartRecorder::new();
    let f = foreman(rec.clone());
    f.apply_manifest(&manifest(vec![quadlet("/etc/app/app.conf", "v1")]))
        .unwrap();
    assert!(
        rec.restarts().is_empty(),
        "host config golem wrote outside a unit dir is never a restart"
    );
}

#[test]
fn a_service_file_under_the_systemd_dir_restarts_itself() {
    let rec = RestartRecorder::new();
    let f = foreman(rec.clone());
    f.apply_manifest(&manifest(vec![quadlet(
        "/etc/systemd/system/api.service",
        "v1",
    )]))
    .unwrap();
    assert_eq!(rec.restarts(), vec!["api.service".to_string()]);
}
