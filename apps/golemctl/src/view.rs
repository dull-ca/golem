//! The pure view over [`ApplyModel`] (ADR 0033 §3, §3b–§3c) — the nested unit
//! tree a person watches while an apply runs. A `logs: <dir>` header sits above
//! the tree from the first frame (§3b); beneath it the model's flat units are
//! rebuilt into a real tree ([`crate::tree`]) whose branch nodes carry an
//! aggregated spinner/mark and whose leaves carry their glyph rows. Each active
//! leaf streams its recent log lines; host-root events collect below the tree.
//!
//! [`render_to_string`] is the tested surface; [`UnitTree`] is the same tree
//! mounted in the live loop, its active marks animated by [`Spinner`]. Both draw
//! from one flat [`Line`] list ([`lines`]) so the static and animated renders
//! never drift, and both pass it through [`fit`] so a tall fleet stays inside the
//! terminal viewport (settled subtrees collapse to their branch row, then
//! settled leaf rows are trimmed before active ones).
//!
//! The marks:
//! - `✓` applied · `·` unchanged · `↩` rolled back · `✗` failed
//! - the spinner frame — active (pending / in progress)

use std::sync::{Arc, Mutex};

use iocraft::prelude::*;

use crate::model::{ApplyModel, GlyphRow};
use crate::poll::GlyphState;
use crate::tree::{build, BranchState, TreeNode};

// The fallback terminal geometry when the pty reports a degenerate size — a
// sizeless pty (a plain pipe promoted to a TTY, a detached session) reports 0×0,
// which would collapse the tree to nothing or trip the viewport guard. 80×24 is
// the conventional default terminal.
pub const DEFAULT_COLS: u16 = 80;
pub const DEFAULT_ROWS: u16 = 24;

// Floor a reported terminal size: a zero (or absent) width or height falls back
// to the conventional 80×24 so a sizeless pty renders sanely (ADR 0033 §3). Each
// axis is floored independently — a terminal that reports a real width but no
// height keeps its width.
pub fn resolve_terminal_size(width: u16, height: u16) -> (u16, u16) {
    let width = if width == 0 { DEFAULT_COLS } else { width };
    let height = if height == 0 { DEFAULT_ROWS } else { height };
    (width, height)
}

fn resolve_terminal_size_from((width, height): (u16, u16)) -> (u16, u16) {
    resolve_terminal_size(width, height)
}

pub const CHECKMARK: &str = "✓";
pub const XMARK: &str = "✗";
pub const ROLLED_BACK: &str = "↩";
pub const UNCHANGED: &str = "·";
pub const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
// Log lines shown under the active unit — the tail of its ring, matching
// devenv-tui's "recent lines under the active node" rule.
const ACTIVE_LOG_LINES: usize = 5;
// Host-root events (no matching leaf unit) shown in the top-level log region.
const ROOT_LOG_LINES: usize = 5;

pub fn glyph_mark(state: GlyphState) -> &'static str {
    match state {
        GlyphState::Applied => CHECKMARK,
        GlyphState::Unchanged => UNCHANGED,
        GlyphState::RolledBack => ROLLED_BACK,
        GlyphState::Failed => XMARK,
        GlyphState::Pending | GlyphState::InProgress => SPINNER_FRAMES[0],
    }
}

pub fn branch_mark(state: BranchState) -> &'static str {
    match state {
        BranchState::Applied => CHECKMARK,
        BranchState::Unchanged => UNCHANGED,
        BranchState::RolledBack => ROLLED_BACK,
        BranchState::Failed => XMARK,
        BranchState::Active => SPINNER_FRAMES[0],
    }
}

// Which of the currently-spinning rows is the actual locus of work (Dr. Dub's
// "only one spinner fully active" reading): the deepest active row — a leaf
// glyph in progress — is `Primary`; every active row above it (its owning
// branch, and that branch's ancestors) is `Folded`, since a branch spins only
// because a descendant is working, never because it is doing the work itself.
// Two leaves genuinely running in parallel each carry their own `Primary`
// glyph row; it is always the ancestor chain that dims, never a sibling.
// Settled rows carry `Primary` too — the field only discriminates spinner
// brightness, and a settled mark isn't a spinner — so it is inert there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Emphasis {
    #[default]
    Primary,
    Folded,
}

// One rendered line of the tree, tagged with the facts both the static and the
// animated render need: its indent depth, whether its mark should spin,
// whether it belongs to a settled subtree, and — for a `Log` line — whether it
// is the host-root region rather than an active unit's tail (so `fit` can
// order its trim passes correctly).
pub enum Line {
    Branch {
        depth: usize,
        label: String,
        active: bool,
        mark: &'static str,
        settled: bool,
        emphasis: Emphasis,
    },
    Glyph {
        depth: usize,
        row: GlyphRow,
        settled: bool,
        emphasis: Emphasis,
    },
    // A buildkit-style tail line (ADR 0033 §3d): one of the last few `cmd`
    // lines under an active glyph, rendered dim and indented under the glyph
    // row. Dropped first by `fit` — it is texture, not the settled record.
    CmdTail {
        depth: usize,
        text: String,
    },
    Log {
        depth: usize,
        text: String,
        host: bool,
    },
    Plain {
        text: String,
    },
}

fn indent(depth: usize) -> String {
    "  ".repeat(depth)
}

// A `Folded` row (an ancestor spinning only because a descendant works) dims
// to grey; a `Primary` row keeps the terminal's default foreground and
// weight, so exactly one spinner per working leaf reads as the fully active
// one.
fn folded_style(emphasis: Emphasis) -> (Option<Color>, Weight) {
    match emphasis {
        Emphasis::Primary => (None, Weight::Normal),
        Emphasis::Folded => (Some(Color::DarkGrey), Weight::Light),
    }
}

// Log and cmd-tail lines read as quieter than the tree rows themselves: dim
// and neutral white, carrying no hue, so they never compete with a spinner or
// a settled mark for attention.
const LOG_COLOR: Color = Color::White;
const LOG_WEIGHT: Weight = Weight::Light;

fn glyph_suffix(g: &GlyphRow) -> String {
    let mut suffix = format!(" {}", g.glyph_key);
    // The countdown is the projection's server-computed `next_retry_in_ms` — the
    // one in-memory seam the WAL cannot carry (ADR 0033 §2) — present only while
    // golemd has a retry scheduled, so it rides an in-progress row, never a
    // terminal ✗.
    if let Some(ms) = g.next_retry_in_ms {
        suffix.push_str(&format!("  (retry in {ms}ms)"));
    }
    suffix
}

// Walk the tree into a flat, depth-tagged line list. A branch renders its
// aggregated mark and label; a leaf renders its glyph rows and, while active,
// the tail of its log ring. Settled subtrees still emit their branch row (a
// person sees the tree settle) — `fit` decides whether their interior survives
// the viewport.
fn push_node(lines: &mut Vec<Line>, node: &TreeNode, depth: usize) {
    let settled = node.state.is_settled();
    let active = node.state.is_active();
    // A branch spins only because a descendant is working — it is never
    // itself the leaf doing the work — so an active branch row always folds,
    // whether it owns the working glyph directly or the work is several
    // levels below. The glyph row is where `Primary` lives.
    lines.push(Line::Branch {
        depth,
        label: node.path.join(" / "),
        active,
        mark: branch_mark(node.state),
        settled,
        emphasis: if active {
            Emphasis::Folded
        } else {
            Emphasis::Primary
        },
    });
    if let Some(unit) = node.leaf {
        for g in &unit.glyphs {
            let active = matches!(g.state, GlyphState::Pending | GlyphState::InProgress);
            lines.push(Line::Glyph {
                depth: depth + 1,
                row: g.clone(),
                settled: !active,
                emphasis: Emphasis::Primary,
            });
            if active {
                for line in &g.cmd_tail {
                    lines.push(Line::CmdTail {
                        depth: depth + 2,
                        text: line.clone(),
                    });
                }
            }
        }
        if node.state.is_active() {
            let start = unit.logs.len().saturating_sub(ACTIVE_LOG_LINES);
            for log in unit.logs.iter().skip(start) {
                lines.push(Line::Log {
                    depth: depth + 1,
                    text: log.clone(),
                    host: false,
                });
            }
        }
    }
    for child in &node.children {
        push_node(lines, child, depth + 1);
    }
}

pub fn lines(model: &ApplyModel) -> Vec<Line> {
    let mut lines: Vec<Line> = Vec::new();
    if let Some(dir) = &model.log_dir {
        lines.push(Line::Plain {
            text: format!("logs: {}/", dir.display()),
        });
    }
    let tree = build(&model.units);
    for root in &tree {
        push_node(&mut lines, root, 0);
    }
    if !model.root_logs.is_empty() {
        lines.push(Line::Plain {
            text: "host".into(),
        });
        let start = model.root_logs.len().saturating_sub(ROOT_LOG_LINES);
        for log in model.root_logs.iter().skip(start) {
            lines.push(Line::Log {
                depth: 1,
                text: log.clone(),
                host: true,
            });
        }
    }
    lines
}

// Keep the frame inside `height` rows so iocraft's height≥viewport `Clear::All`
// guard never fires on a real fleet, trimming in the order a person can afford
// to lose rows: settled glyph interiors first (a settled leaf's outcome is
// still named by its branch mark), then settled branch labels, and only then
// the active unit's live log tail — the one thing an operator watching an
// in-flight apply actually needs — with the host-root log region going last of
// all. `height == 0` means unbounded (the render_to_string default and any
// terminal that reports no size).
pub fn fit(mut lines: Vec<Line>, height: usize) -> Vec<Line> {
    if height == 0 || lines.len() <= height {
        return lines;
    }
    lines.retain(|l| !is_settled_interior(l));
    if lines.len() <= height {
        return lines;
    }
    lines = drop_from_bottom(lines, height, |l| {
        matches!(l, Line::Branch { settled: true, .. })
    });
    if lines.len() <= height {
        return lines;
    }
    lines = drop_from_bottom(lines, height, |l| matches!(l, Line::CmdTail { .. }));
    if lines.len() <= height {
        return lines;
    }
    lines = drop_from_bottom(lines, height, |l| {
        matches!(l, Line::Log { host: false, .. })
    });
    if lines.len() <= height {
        return lines;
    }
    drop_from_bottom(lines, height, |l| matches!(l, Line::Log { host: true, .. }))
}

// Drop rows matching `is_droppable` from the bottom up until `lines` fits
// `height` or no more matching rows remain, preserving the order of everything
// kept.
fn drop_from_bottom(
    lines: Vec<Line>,
    height: usize,
    is_droppable: impl Fn(&Line) -> bool,
) -> Vec<Line> {
    let mut over = lines.len().saturating_sub(height);
    let mut kept: Vec<Line> = Vec::with_capacity(lines.len());
    for l in lines.into_iter().rev() {
        if over > 0 && is_droppable(&l) {
            over -= 1;
        } else {
            kept.push(l);
        }
    }
    kept.reverse();
    kept
}

fn is_settled_interior(l: &Line) -> bool {
    matches!(l, Line::Glyph { settled: true, .. })
}

fn static_line(l: &Line) -> AnyElement<'static> {
    match l {
        Line::Branch {
            depth,
            label,
            mark,
            emphasis,
            ..
        } => {
            let (color, weight) = folded_style(*emphasis);
            element!(Text(content: format!("{}{} {}", indent(*depth), mark, label), color: color, weight: weight))
                .into_any()
        }
        Line::Glyph {
            depth,
            row,
            emphasis,
            ..
        } => {
            let (color, weight) = folded_style(*emphasis);
            element!(Text(content: format!("{}{}{}", indent(*depth), glyph_mark(row.state), glyph_suffix(row)), color: color, weight: weight))
                .into_any()
        }
        Line::CmdTail { depth, text } => {
            element!(Text(content: format!("{}{}", indent(*depth), text), color: LOG_COLOR, weight: LOG_WEIGHT))
                .into_any()
        }
        Line::Log { depth, text, .. } => {
            element!(Text(content: format!("{}{}", indent(*depth + 1), text), color: LOG_COLOR, weight: LOG_WEIGHT))
                .into_any()
        }
        Line::Plain { text } => element!(Text(content: text.clone())).into_any(),
    }
}

fn animated_line(l: &Line) -> AnyElement<'static> {
    match l {
        Line::Branch {
            depth,
            label,
            active,
            mark,
            emphasis,
            ..
        } => {
            let (color, weight) = folded_style(*emphasis);
            element! {
                View(flex_direction: FlexDirection::Row) {
                    Text(content: indent(*depth))
                    StatusIndicator(mark: *mark, active: *active, emphasis: *emphasis)
                    Text(content: format!(" {label}"), color: color, weight: weight)
                }
            }
            .into_any()
        }
        Line::Glyph {
            depth,
            row,
            emphasis,
            ..
        } => {
            let active = matches!(row.state, GlyphState::Pending | GlyphState::InProgress);
            let (color, weight) = folded_style(*emphasis);
            element! {
                View(flex_direction: FlexDirection::Row) {
                    Text(content: indent(*depth))
                    StatusIndicator(mark: glyph_mark(row.state), active: active, emphasis: *emphasis)
                    Text(content: glyph_suffix(row), color: color, weight: weight)
                }
            }
            .into_any()
        }
        Line::CmdTail { depth, text } => {
            element!(Text(content: format!("{}{}", indent(*depth), text), color: LOG_COLOR, weight: LOG_WEIGHT))
                .into_any()
        }
        Line::Log { depth, text, .. } => {
            element!(Text(content: format!("{}{}", indent(*depth + 1), text), color: LOG_COLOR, weight: LOG_WEIGHT))
                .into_any()
        }
        Line::Plain { text } => element!(Text(content: text.clone())).into_any(),
    }
}

// The static-mark view: the tested surface and the exact tree the live
// `UnitTree` mirrors, with `glyph_mark`/`branch_mark` emitting the still spinner
// frame the animated components replace at runtime.
pub fn view(model: &ApplyModel) -> impl Into<AnyElement<'static>> {
    let rows: Vec<AnyElement<'static>> = lines(model).iter().map(static_line).collect();
    element! {
        View(flex_direction: FlexDirection::Column) {
            #(rows)
        }
    }
}

pub fn render_to_string(model: &ApplyModel, width: usize) -> String {
    render_to_string_bounded(model, width, 0)
}

pub fn render_to_string_bounded(model: &ApplyModel, width: usize, height: usize) -> String {
    let rows: Vec<AnyElement<'static>> =
        fit(lines(model), height).iter().map(static_line).collect();
    let mut element: AnyElement<'static> = element! {
        View(flex_direction: FlexDirection::Column) {
            #(rows)
        }
    }
    .into();
    element.render(Some(width)).to_string()
}

// The live tree, mirroring `view` row-for-row but driving each active mark
// through the self-animating `StatusIndicator`. Mounted by `apply::run_tui`
// under a `render_loop`, it reads the shared model each frame and re-renders at
// the width and height iocraft reports for the terminal, so a tall fleet stays
// inside the viewport (`fit`) and no frame trips the full-screen clear.
#[component]
pub fn UnitTree(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let model = hooks.use_context::<Arc<Mutex<ApplyModel>>>();
    let (width, height) = resolve_terminal_size_from(hooks.use_terminal_size());

    // Stay strictly under the reported viewport: a frame whose height equals the
    // terminal's trips iocraft's `Clear::All` guard, which the inline loop must
    // never hit. One row of headroom keeps the diff-based repaint inline.
    let budget = (height as usize).saturating_sub(1);
    let rows: Vec<AnyElement<'static>> = match model.lock() {
        Ok(model) => fit(lines(&model), budget)
            .iter()
            .map(animated_line)
            .collect(),
        Err(_) => Vec::new(),
    };

    element! {
        View(width: width, flex_direction: FlexDirection::Column) {
            #(rows)
        }
    }
}

#[derive(Default, Props)]
pub struct SpinnerProps {
    pub emphasis: Emphasis,
}

/// Self-animating spinner: advances its own frame on an ~80ms timer, so the
/// mark spins between the ~1s polls that refresh the model. A `Folded`
/// spinner (an ancestor row spinning only because a descendant is working)
/// renders dim and grey, obviously subordinate to the one `Primary` spinner
/// at the working leaf.
#[component]
pub fn Spinner(mut hooks: Hooks, props: &SpinnerProps) -> impl Into<AnyElement<'static>> {
    let mut frame = hooks.use_state(|| 0usize);
    hooks.use_future(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
            let Some(v) = frame.try_get() else { break };
            frame.set((v + 1) % SPINNER_FRAMES.len());
        }
    });
    let (color, weight) = folded_style(props.emphasis);
    element!(Text(content: SPINNER_FRAMES[frame.get()], color: color, weight: weight))
}

#[derive(Default, Props)]
pub struct StatusIndicatorProps {
    pub mark: &'static str,
    pub active: bool,
    pub emphasis: Emphasis,
}

#[component]
pub fn StatusIndicator(
    _hooks: Hooks,
    props: &StatusIndicatorProps,
) -> impl Into<AnyElement<'static>> {
    if props.active {
        element!(Spinner(emphasis: props.emphasis)).into_any()
    } else {
        element!(Text(content: props.mark)).into_any()
    }
}
