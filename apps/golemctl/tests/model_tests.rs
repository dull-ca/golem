use golemctl::model::{ApplyModel, UnitState, CMD_TAIL_LINES};
use golemctl::poll::{Event, EventKind, GlyphProgress, GlyphState, Phase, Progress, UnitProgress};
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
    tagged_event(seq, EventKind::Lifecycle, unit, key, msg)
}

fn cmd_event(seq: u64, unit: &[&str], key: &str, msg: &str) -> Event {
    tagged_event(seq, EventKind::Cmd, unit, key, msg)
}

fn tagged_event(seq: u64, kind: EventKind, unit: &[&str], key: &str, msg: &str) -> Event {
    Event {
        seq,
        at: "2026-07-26T00:00:00Z".into(),
        level: "info".into(),
        kind,
        unit_path: unit.iter().map(|s| s.to_string()).collect(),
        glyph_key: key.into(),
        message: msg.into(),
    }
}

fn in_progress_unit(seq_base: u64, cmd_lines: &[&str]) -> Progress {
    Progress {
        reconcile_id: 1,
        phase: Phase::Enacting,
        units: vec![UnitProgress {
            unit_path: vec!["scaly".into(), "a".into()],
            glyphs: vec![glyph("apt:podman", GlyphState::InProgress)],
        }],
        events: cmd_lines
            .iter()
            .enumerate()
            .map(|(i, l)| cmd_event(seq_base + i as u64, &["scaly", "a"], "apt:podman", l))
            .collect(),
        cursor: seq_base + cmd_lines.len() as u64,
        report: None,
    }
}

fn podman_tail(m: &ApplyModel) -> Vec<String> {
    m.units[0].glyphs[0].cmd_tail.iter().cloned().collect()
}

#[test]
fn cmd_events_roll_a_bounded_per_glyph_tail_and_evict_the_oldest() {
    let mut m = ApplyModel::new();
    m.apply_progress(in_progress_unit(
        1,
        &["Unpacking podman ...", "Setting up conmon ...", "Processing triggers ..."],
    ));
    assert_eq!(
        podman_tail(&m),
        vec!["Unpacking podman ...", "Setting up conmon ...", "Processing triggers ..."]
    );

    m.apply_progress(in_progress_unit(4, &["Setting up podman ..."]));
    assert_eq!(podman_tail(&m).len(), CMD_TAIL_LINES);
    assert_eq!(
        podman_tail(&m),
        vec!["Setting up conmon ...", "Processing triggers ...", "Setting up podman ..."],
        "the 4th line evicts the 1st"
    );
}

#[test]
fn a_cmd_tail_collapses_when_the_glyph_settles() {
    let mut m = ApplyModel::new();
    m.apply_progress(in_progress_unit(1, &["Unpacking podman ...", "Setting up podman ..."]));
    assert!(!podman_tail(&m).is_empty());

    m.apply_progress(Progress {
        reconcile_id: 1,
        phase: Phase::Settled,
        units: vec![UnitProgress {
            unit_path: vec!["scaly".into(), "a".into()],
            glyphs: vec![glyph("apt:podman", GlyphState::Applied)],
        }],
        events: vec![],
        cursor: 3,
        report: Some(serde_json::json!({ "outcome": "settled" })),
    });
    assert!(
        podman_tail(&m).is_empty(),
        "the tail collapses once the glyph settles"
    );
}

#[test]
fn cmd_events_do_not_land_in_the_lifecycle_log_region() {
    let mut m = ApplyModel::new();
    m.apply_progress(in_progress_unit(1, &["Unpacking podman ..."]));
    assert!(
        m.units[0].logs.is_empty(),
        "a cmd line is a glyph tail, not a unit lifecycle log"
    );
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
fn an_event_at_the_host_root_lands_in_the_top_level_log() {
    let mut m = ApplyModel::new();
    m.apply_progress(Progress {
        reconcile_id: 1,
        phase: Phase::Enacting,
        units: vec![UnitProgress {
            unit_path: vec!["scaly".into(), "a".into()],
            glyphs: vec![glyph("apt:podman", GlyphState::InProgress)],
        }],
        events: vec![event(
            9,
            &["scaly"],
            "reconcile",
            "reconcile panicked: poisoned lock",
        )],
        cursor: 9,
        report: None,
    });
    assert_eq!(m.units.len(), 1);
    assert!(m.units[0].logs.is_empty());
    assert_eq!(m.root_logs.len(), 1);
    assert!(m.root_logs[0].contains("reconcile panicked"));
}

#[test]
fn the_view_renders_root_events_in_a_top_level_log_region() {
    let mut m = ApplyModel::new();
    m.apply_progress(Progress {
        reconcile_id: 1,
        phase: Phase::Enacting,
        units: vec![UnitProgress {
            unit_path: vec!["scaly".into(), "a".into()],
            glyphs: vec![glyph("apt:podman", GlyphState::InProgress)],
        }],
        events: vec![event(
            9,
            &["scaly"],
            "reconcile",
            "reconcile panicked: poisoned lock",
        )],
        cursor: 9,
        report: None,
    });
    let out = view::render_to_string(&m, 100);
    assert!(out.contains("reconcile panicked"));
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
