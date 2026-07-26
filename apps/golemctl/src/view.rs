//! The pure view over [`ApplyModel`] (ADR 0033 §3) — the unit tree a person
//! watches while an apply runs. Each unit is a header row with its mark, then a
//! row per glyph; the recent log lines of the *active* unit stream in beneath
//! it, and any host-root events (those matching no leaf unit) collect in a
//! top-level log region below the tree. [`render_to_string`] is the tested
//! surface; [`MainView`] is the same tree mounted in the live render loop, with
//! its marks animated by the self-driving [`Spinner`].
//!
//! The marks:
//! - `✓` settled (applied)   · `·` unchanged (a no-op)
//! - `↩` rolled back         · `✗` failed
//! - the spinner frame       — active (pending / in progress)

use std::sync::{Arc, Mutex};

use iocraft::prelude::*;

use crate::model::{ApplyModel, GlyphRow, UnitNode, UnitState};
use crate::poll::GlyphState;

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

pub fn unit_mark(state: UnitState) -> &'static str {
    match state {
        UnitState::Settled => CHECKMARK,
        UnitState::Failed => XMARK,
        UnitState::Active => SPINNER_FRAMES[0],
    }
}

fn glyph_row(g: &GlyphRow) -> AnyElement<'static> {
    let mut line = format!("  {} {}", glyph_mark(g.state), g.glyph_key);
    if let Some(ms) = g.next_retry_in_ms {
        // Server-computed countdown from the projection's `next_retry_in_ms`;
        // present only on an in-progress row awaiting its next round, never on a
        // failed one — a `✗` glyph has no countdown.
        line.push_str(&format!("  (retry in {ms}ms)"));
    }
    element!(Text(content: line)).into_any()
}

fn animated_glyph_row(g: &GlyphRow) -> AnyElement<'static> {
    let active = matches!(g.state, GlyphState::Pending | GlyphState::InProgress);
    let mut suffix = format!(" {}", g.glyph_key);
    if let Some(ms) = g.next_retry_in_ms {
        suffix.push_str(&format!("  (retry in {ms}ms)"));
    }
    element! {
        View(flex_direction: FlexDirection::Row) {
            Text(content: "  ")
            StatusIndicator(mark: glyph_mark(g.state), active: active)
            Text(content: suffix)
        }
    }
    .into_any()
}

fn unit_header(unit: &UnitNode) -> String {
    format!("{} {}", unit_mark(unit.state), unit.unit_path.join(" / "))
}

fn active_log_rows(unit: &UnitNode) -> Vec<AnyElement<'static>> {
    if unit.state != UnitState::Active {
        return vec![];
    }
    let start = unit.logs.len().saturating_sub(ACTIVE_LOG_LINES);
    unit.logs
        .iter()
        .skip(start)
        .map(|log| element!(Text(content: format!("    {log}"))).into_any())
        .collect()
}

fn root_log_rows(model: &ApplyModel) -> Vec<AnyElement<'static>> {
    if model.root_logs.is_empty() {
        return vec![];
    }
    let start = model.root_logs.len().saturating_sub(ROOT_LOG_LINES);
    let mut rows = vec![element!(Text(content: "host")).into_any()];
    rows.extend(
        model
            .root_logs
            .iter()
            .skip(start)
            .map(|log| element!(Text(content: format!("    {log}"))).into_any()),
    );
    rows
}

// The static-mark view: the tested surface and the exact tree the live
// `MainView` mirrors, with `glyph_mark`/`unit_mark` emitting the still spinner
// frame the animated components replace at runtime.
pub fn view(model: &ApplyModel) -> impl Into<AnyElement<'static>> {
    let mut rows: Vec<AnyElement<'static>> = Vec::new();
    for unit in &model.units {
        rows.push(element!(Text(content: unit_header(unit))).into_any());
        for g in &unit.glyphs {
            rows.push(glyph_row(g));
        }
        rows.extend(active_log_rows(unit));
    }
    rows.extend(root_log_rows(model));
    element! {
        View(flex_direction: FlexDirection::Column) {
            #(rows)
        }
    }
}

pub fn render_to_string(model: &ApplyModel, width: usize) -> String {
    let mut element: AnyElement<'static> = view(model).into();
    element.render(Some(width)).to_string()
}

// The live tree, mirroring `view` row-for-row but driving each active mark
// through the self-animating `StatusIndicator`. Mounted by `apply::run_tui`
// under a `render_loop`, it reads the shared model each frame and re-renders at
// the width iocraft reports for the terminal.
#[component]
pub fn UnitTree(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let model = hooks.use_context::<Arc<Mutex<ApplyModel>>>();
    let (width, _) = hooks.use_terminal_size();

    let mut rows: Vec<AnyElement<'static>> = Vec::new();
    if let Ok(model) = model.lock() {
        for unit in &model.units {
            rows.push(
                element! {
                    View(flex_direction: FlexDirection::Row) {
                        StatusIndicator(mark: unit_mark(unit.state), active: unit.state == UnitState::Active)
                        Text(content: format!(" {}", unit.unit_path.join(" / ")))
                    }
                }
                .into_any(),
            );
            for g in &unit.glyphs {
                rows.push(animated_glyph_row(g));
            }
            rows.extend(active_log_rows(unit));
        }
        rows.extend(root_log_rows(&model));
    }

    element! {
        View(width: width, flex_direction: FlexDirection::Column) {
            #(rows)
        }
    }
}

/// Self-animating spinner: advances its own frame on an ~80ms timer, so the
/// mark spins between the ~1s polls that refresh the model.
#[component]
pub fn Spinner(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut frame = hooks.use_state(|| 0usize);
    hooks.use_future(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
            let Some(v) = frame.try_get() else { break };
            frame.set((v + 1) % SPINNER_FRAMES.len());
        }
    });
    element!(Text(content: SPINNER_FRAMES[frame.get()]))
}

#[derive(Default, Props)]
pub struct StatusIndicatorProps {
    pub mark: &'static str,
    pub active: bool,
}

#[component]
pub fn StatusIndicator(
    _hooks: Hooks,
    props: &StatusIndicatorProps,
) -> impl Into<AnyElement<'static>> {
    if props.active {
        element!(Spinner).into_any()
    } else {
        element!(Text(content: props.mark)).into_any()
    }
}
