use golemctl::model::{ApplyModel, UnitState};
use golemctl::poll::{Event, GlyphProgress, GlyphState, Phase, Progress, UnitProgress};
use golemctl::view;

fn glyph(key: &str, state: GlyphState) -> GlyphProgress {
    GlyphProgress {
        glyph_key: key.into(),
        action: "install".into(),
        state,
        rounds: 1,
        next_retry_in_ms: None,
    }
}

fn event(seq: u64, unit: &[&str], key: &str, msg: &str) -> Event {
    Event {
        seq,
        at: "2026-07-26T00:00:00Z".into(),
        level: "info".into(),
        unit_path: unit.iter().map(|s| s.to_string()).collect(),
        glyph_key: key.into(),
        message: msg.into(),
    }
}

#[test]
fn applying_progress_builds_the_unit_tree_and_appends_logs() {
    let mut m = ApplyModel::new();
    m.apply_progress(Progress {
        reconcile_id: 1,
        phase: Phase::Enacting,
        units: vec![UnitProgress {
            unit_path: vec!["scaly".into(), "a".into()],
            glyphs: vec![glyph("apt:podman", GlyphState::InProgress)],
        }],
        events: vec![event(1, &["scaly", "a"], "apt:podman", "install apt:podman")],
        cursor: 1,
        report: None,
    });
    assert_eq!(m.units.len(), 1);
    assert_eq!(m.units[0].unit_path, vec!["scaly", "a"]);
    assert!(matches!(m.units[0].state, UnitState::Active));
    assert_eq!(m.units[0].logs.len(), 1);
    assert!(m.units[0].logs[0].contains("install apt:podman"));
    assert_eq!(m.cursor, 1);

    m.apply_progress(Progress {
        reconcile_id: 1,
        phase: Phase::Settled,
        units: vec![UnitProgress {
            unit_path: vec!["scaly".into(), "a".into()],
            glyphs: vec![glyph("apt:podman", GlyphState::Applied)],
        }],
        events: vec![event(2, &["scaly", "a"], "apt:podman", "apt:podman done")],
        cursor: 2,
        report: Some(serde_json::json!({ "outcome": "settled" })),
    });
    assert_eq!(m.units.len(), 1);
    assert!(matches!(m.units[0].state, UnitState::Settled));
    assert_eq!(m.units[0].logs.len(), 2);
    assert!(m.is_settled());
    assert!(m.report.is_some());
}

#[test]
fn the_view_renders_unit_paths_marks_and_active_logs() {
    let mut m = ApplyModel::new();
    m.apply_progress(Progress {
        reconcile_id: 1,
        phase: Phase::Enacting,
        units: vec![
            UnitProgress {
                unit_path: vec!["scaly".into(), "base".into()],
                glyphs: vec![glyph("apt:htop", GlyphState::Applied)],
            },
            UnitProgress {
                unit_path: vec!["scaly".into(), "fishnet-a".into()],
                glyphs: vec![glyph("apt:podman", GlyphState::InProgress)],
            },
        ],
        events: vec![event(1, &["scaly", "fishnet-a"], "apt:podman", "install apt:podman")],
        cursor: 1,
        report: None,
    });
    let out = view::render_to_string(&m, 100);
    assert!(out.contains("scaly / base"));
    assert!(out.contains("scaly / fishnet-a"));
    assert!(out.contains("apt:htop"));
    assert!(out.contains(view::CHECKMARK));
    assert!(out.contains("install apt:podman"));
}

#[test]
fn a_failed_glyph_shows_the_x_mark() {
    let mut m = ApplyModel::new();
    m.apply_progress(Progress {
        reconcile_id: 1,
        phase: Phase::RolledBack,
        units: vec![UnitProgress {
            unit_path: vec!["scaly".into(), "canary".into()],
            glyphs: vec![glyph("systemd:canary.service", GlyphState::Failed)],
        }],
        events: vec![],
        cursor: 0,
        report: Some(serde_json::json!({ "outcome": "partial" })),
    });
    let out = view::render_to_string(&m, 100);
    assert!(out.contains(view::XMARK));
    assert!(out.contains("scaly / canary"));
}
