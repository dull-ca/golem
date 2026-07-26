use golemctl::model::ApplyModel;
use golemctl::poll::{GlyphProgress, GlyphState, Phase, Progress, UnitProgress};
use golemctl::tree::{build, BranchState};

fn glyph(key: &str, state: GlyphState) -> GlyphProgress {
    GlyphProgress {
        glyph_key: key.into(),
        action: "install".into(),
        state,
        rounds: 1,
        next_retry_in_ms: None,
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
fn a_shared_prefix_becomes_one_branch_with_children() {
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
    let tree = build(&m.units);
    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].segment, "scaly");
    assert_eq!(tree[0].children.len(), 2);
    assert!(tree[0].leaf.is_none());
}

#[test]
fn a_branch_spins_while_any_descendant_is_active() {
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
    let tree = build(&m.units);
    assert_eq!(tree[0].state, BranchState::Active);
}

#[test]
fn a_branch_aggregates_to_failed_over_a_rolled_back_sibling() {
    let m = model(vec![
        unit(
            &["scaly", "canary"],
            vec![glyph("systemd:canary.service", GlyphState::Failed)],
        ),
        unit(
            &["scaly", "base"],
            vec![glyph("apt:htop", GlyphState::RolledBack)],
        ),
    ]);
    let tree = build(&m.units);
    assert_eq!(tree[0].state, BranchState::Failed);
}

#[test]
fn a_wholly_rolled_back_subtree_aggregates_to_rolled_back() {
    let m = model(vec![
        unit(
            &["scaly", "a"],
            vec![glyph("apt:podman", GlyphState::RolledBack)],
        ),
        unit(
            &["scaly", "b"],
            vec![glyph("apt:htop", GlyphState::RolledBack)],
        ),
    ]);
    let tree = build(&m.units);
    assert_eq!(tree[0].state, BranchState::RolledBack);
}

#[test]
fn a_settled_branch_resolves_applied_when_any_work_happened() {
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
    let tree = build(&m.units);
    assert_eq!(tree[0].state, BranchState::Applied);
}

#[test]
fn a_settled_branch_resolves_unchanged_when_every_descendant_was_a_noop() {
    let m = model(vec![
        unit(
            &["scaly", "a"],
            vec![glyph("apt:podman", GlyphState::Unchanged)],
        ),
        unit(
            &["scaly", "b"],
            vec![glyph("apt:htop", GlyphState::Unchanged)],
        ),
    ]);
    let tree = build(&m.units);
    assert_eq!(tree[0].state, BranchState::Unchanged);
}

#[test]
fn a_three_level_branch_spins_while_a_deep_child_runs() {
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
    let tree = build(&m.units);
    assert_eq!(tree[0].segment, "scaly");
    assert_eq!(tree[0].state, BranchState::Active);
    let pod = &tree[0].children[0];
    assert_eq!(pod.segment, "pod");
    assert_eq!(pod.state, BranchState::Active);
    assert_eq!(pod.children.len(), 2);
}

#[test]
fn a_single_leaf_root_is_a_one_node_tree() {
    let m = model(vec![unit(
        &["only"],
        vec![glyph("apt:podman", GlyphState::Applied)],
    )]);
    let tree = build(&m.units);
    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].segment, "only");
    assert!(tree[0].leaf.is_some());
    assert_eq!(tree[0].state, BranchState::Applied);
}

#[test]
fn the_removes_group_is_an_ordinary_branch() {
    let m = model(vec![unit(
        &["<removes>"],
        vec![glyph("apt:old", GlyphState::Applied)],
    )]);
    let tree = build(&m.units);
    assert_eq!(tree[0].segment, "<removes>");
    assert!(tree[0].leaf.is_some());
}
