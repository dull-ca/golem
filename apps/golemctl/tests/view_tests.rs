use std::path::PathBuf;

use golemctl::model::ApplyModel;
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

fn cmd_event(unit: &[&str], key: &str, msg: &str) -> Event {
    Event {
        seq: 1,
        at: "2026-07-26T00:00:00Z".into(),
        level: "info".into(),
        kind: EventKind::Cmd,
        unit_path: unit.iter().map(|s| s.to_string()).collect(),
        glyph_key: key.into(),
        message: msg.into(),
    }
}

fn progress_with(units: Vec<UnitProgress>, events: Vec<Event>) -> Progress {
    Progress {
        reconcile_id: 1,
        phase: Phase::Enacting,
        units,
        events,
        cursor: 1,
        report: None,
    }
}

fn unit(path: &[&str], glyphs: Vec<GlyphProgress>) -> UnitProgress {
    UnitProgress {
        unit_path: path.iter().map(|s| s.to_string()).collect(),
        glyphs,
    }
}

fn model(units: Vec<UnitProgress>) -> ApplyModel {
    let mut m = ApplyModel::new();
    m.apply_progress(Progress {
        reconcile_id: 1,
        phase: Phase::Enacting,
        units,
        events: vec![],
        cursor: 0,
        report: None,
    });
    m
}

#[test]
fn the_header_names_the_log_directory_above_the_tree() {
    let mut m = model(vec![unit(
        &["scaly", "a"],
        vec![glyph("apt:podman", GlyphState::InProgress)],
    )]);
    m.log_dir = Some(PathBuf::from("/tmp/golemctl/apply-42"));
    let out = view::render_to_string(&m, 100);
    let header = out.lines().next().unwrap();
    assert!(header.contains("logs: /tmp/golemctl/apply-42/"));
}

#[test]
fn the_header_renders_before_any_unit_exists() {
    let mut m = ApplyModel::new();
    m.log_dir = Some(PathBuf::from("/tmp/golemctl/apply-7"));
    let out = view::render_to_string(&m, 100);
    assert!(out.contains("logs: /tmp/golemctl/apply-7/"));
}

#[test]
fn a_branch_row_renders_above_its_indented_children() {
    let m = model(vec![
        unit(
            &["scaly", "base"],
            vec![glyph("apt:htop", GlyphState::Applied)],
        ),
        unit(
            &["scaly", "fishnet-a"],
            vec![glyph("apt:podman", GlyphState::InProgress)],
        ),
    ]);
    let out = view::render_to_string(&m, 100);
    let lines: Vec<&str> = out.lines().collect();
    let branch = lines.iter().position(|l| l.trim() == "⠋ scaly").unwrap();
    let base = lines
        .iter()
        .position(|l| l.contains("scaly / base"))
        .unwrap();
    assert!(branch < base);
    assert!(lines[base].starts_with("  "));
}

#[test]
fn a_mid_flight_branch_shows_the_spinner_frame() {
    let m = model(vec![
        unit(
            &["scaly", "pod", "web"],
            vec![glyph("systemd:web.service", GlyphState::InProgress)],
        ),
        unit(
            &["scaly", "pod", "db"],
            vec![glyph("systemd:db.service", GlyphState::Applied)],
        ),
    ]);
    let out = view::render_to_string(&m, 100);
    assert!(out.contains(view::SPINNER_FRAMES[0]));
    assert!(out.contains(view::CHECKMARK));
}

#[test]
fn a_failed_leaf_bubbles_the_x_to_its_branch() {
    let m = {
        let mut m = ApplyModel::new();
        m.apply_progress(Progress {
            reconcile_id: 1,
            phase: Phase::RolledBack,
            units: vec![unit(
                &["scaly", "canary"],
                vec![glyph("systemd:canary.service", GlyphState::Failed)],
            )],
            events: vec![],
            cursor: 0,
            report: None,
        });
        m
    };
    let out = view::render_to_string(&m, 100);
    let branch = out.lines().find(|l| l.trim() == "✗ scaly").unwrap();
    assert!(branch.contains(view::XMARK));
}

#[test]
fn a_settled_branch_resolves_the_checkmark() {
    let m = model(vec![
        unit(
            &["scaly", "a"],
            vec![glyph("apt:podman", GlyphState::Applied)],
        ),
        unit(
            &["scaly", "b"],
            vec![glyph("apt:htop", GlyphState::Unchanged)],
        ),
    ]);
    let out = view::render_to_string(&m, 100);
    assert!(out.lines().any(|l| l.trim() == "✓ scaly"));
}

// A tall fleet must render inside the viewport so the inline loop's
// height≥viewport `Clear::All` guard never fires (ADR 0033 §3c / the 21f218d
// concern). Settled subtrees collapse to their branch row before active ones are
// touched.
#[test]
fn a_tall_tree_collapses_settled_subtrees_to_fit_the_viewport() {
    let mut units = Vec::new();
    for i in 0..20 {
        units.push(unit(
            &["scaly", &format!("settled-{i}")],
            vec![glyph("apt:pkg", GlyphState::Applied)],
        ));
    }
    units.push(unit(
        &["scaly", "running"],
        vec![glyph("apt:podman", GlyphState::InProgress)],
    ));
    let m = model(units);

    let bounded = view::render_to_string_bounded(&m, 100, 12);
    let n = bounded.lines().filter(|l| !l.trim().is_empty()).count();
    assert!(n <= 12, "bounded frame had {n} non-empty lines");
    // The active leaf and its glyph survive the trim.
    assert!(bounded.contains("scaly / running"));
    assert!(bounded.contains("apt:podman"));
    // A settled leaf keeps its branch row but drops its glyph interior.
    let unbounded = view::render_to_string(&m, 100);
    assert!(unbounded.contains("scaly / settled-0"));
}

// The first trim pass (settled interiors) must not take the active unit's own
// log tail with it: `fit`'s stated precedence is settled interiors, then
// settled branch labels, and only then an active unit's live logs — never the
// other way around, or an operator watching an in-flight apply loses the one
// thing they're watching while distant settled rows survive.
#[test]
fn fit_keeps_the_active_units_log_tail_over_settled_rows() {
    let mut units = Vec::new();
    for i in 0..20 {
        units.push(unit(
            &["scaly", &format!("settled-{i}")],
            vec![glyph("apt:pkg", GlyphState::Applied)],
        ));
    }
    units.push(unit(
        &["scaly", "running"],
        vec![glyph("apt:podman", GlyphState::InProgress)],
    ));
    let mut m = model(units);
    m.apply_progress(Progress {
        reconcile_id: 1,
        phase: Phase::Enacting,
        units: vec![],
        events: vec![Event {
            seq: 1,
            at: "2026-07-26T00:00:00Z".into(),
            level: "info".into(),
            kind: EventKind::Lifecycle,
            unit_path: vec!["scaly".into(), "running".into()],
            glyph_key: "apt:podman".into(),
            message: "unpacking podman".into(),
        }],
        cursor: 1,
        report: None,
    });

    let bounded = view::render_to_string_bounded(&m, 100, 12);
    let n = bounded.lines().filter(|l| !l.trim().is_empty()).count();
    assert!(n <= 12, "bounded frame had {n} non-empty lines");
    assert!(
        bounded.contains("unpacking podman"),
        "active unit's log tail was trimmed before settled rows:\n{bounded}"
    );
}

#[test]
fn an_active_glyph_renders_its_cmd_tail_under_the_row() {
    let mut m = model(vec![unit(
        &["scaly", "a"],
        vec![glyph("apt:podman", GlyphState::InProgress)],
    )]);
    m.apply_progress(progress_with(
        vec![],
        vec![
            cmd_event(
                &["scaly", "a"],
                "apt:podman",
                "Unpacking podman (4.3.1) ...",
            ),
            cmd_event(&["scaly", "a"], "apt:podman", "Setting up conmon ..."),
        ],
    ));
    let out = view::render_to_string(&m, 100);
    assert!(out.contains("Unpacking podman (4.3.1) ..."));
    assert!(out.contains("Setting up conmon ..."));
}

#[test]
fn the_cmd_tail_collapses_when_the_glyph_settles() {
    let mut m = model(vec![unit(
        &["scaly", "a"],
        vec![glyph("apt:podman", GlyphState::InProgress)],
    )]);
    m.apply_progress(progress_with(
        vec![],
        vec![cmd_event(
            &["scaly", "a"],
            "apt:podman",
            "Unpacking podman ...",
        )],
    ));
    assert!(view::render_to_string(&m, 100).contains("Unpacking podman ..."));

    m.apply_progress(Progress {
        reconcile_id: 1,
        phase: Phase::Settled,
        units: vec![unit(
            &["scaly", "a"],
            vec![glyph("apt:podman", GlyphState::Applied)],
        )],
        events: vec![],
        cursor: 2,
        report: Some(serde_json::json!({ "outcome": "settled" })),
    });
    let out = view::render_to_string(&m, 100);
    assert!(
        !out.contains("Unpacking podman ..."),
        "the tail disappears once the glyph settles:\n{out}"
    );
    assert!(out.contains(view::CHECKMARK));
}

#[test]
fn lifecycle_log_region_is_unaffected_by_cmd_tails() {
    let mut m = model(vec![unit(
        &["scaly", "a"],
        vec![glyph("apt:podman", GlyphState::InProgress)],
    )]);
    m.apply_progress(progress_with(
        vec![],
        vec![
            Event {
                seq: 1,
                at: "t".into(),
                level: "info".into(),
                kind: EventKind::Lifecycle,
                unit_path: vec!["scaly".into(), "a".into()],
                glyph_key: "apt:podman".into(),
                message: "install apt:podman".into(),
            },
            cmd_event(&["scaly", "a"], "apt:podman", "Unpacking podman ..."),
        ],
    ));
    let out = view::render_to_string(&m, 100);
    assert!(
        out.contains("install apt:podman"),
        "lifecycle line still renders"
    );
    assert!(out.contains("Unpacking podman ..."), "cmd tail renders too");
}

#[test]
fn resolve_terminal_size_floors_degenerate_dimensions() {
    assert_eq!(
        view::resolve_terminal_size(0, 0),
        (view::DEFAULT_COLS, view::DEFAULT_ROWS),
        "a sizeless pty falls back to 80x24"
    );
    assert_eq!(
        view::resolve_terminal_size(0, 40),
        (view::DEFAULT_COLS, 40),
        "each axis floors independently"
    );
    assert_eq!(
        view::resolve_terminal_size(120, 0),
        (120, view::DEFAULT_ROWS)
    );
    assert_eq!(
        view::resolve_terminal_size(120, 40),
        (120, 40),
        "a real size is left untouched"
    );
}

#[test]
fn height_zero_is_unbounded() {
    let mut units = Vec::new();
    for i in 0..30 {
        units.push(unit(
            &["scaly", &format!("u-{i}")],
            vec![glyph("apt:pkg", GlyphState::Applied)],
        ));
    }
    let m = model(units);
    let out = view::render_to_string_bounded(&m, 100, 0);
    assert!(out.lines().filter(|l| !l.trim().is_empty()).count() > 30);
}
