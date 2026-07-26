//! The pure view over [`ApplyModel`] (ADR 0033 §3) — the unit tree a person
//! watches while an apply runs. Each unit is a header row with its mark, then a
//! row per glyph; the recent log lines of the *active* unit stream in beneath
//! it. [`render_to_string`] is the tested surface and the render the run loop
//! currently drives.
//!
//! The marks:
//! - `✓` settled (applied)   · `·` unchanged (a no-op)
//! - `↩` rolled back         · `✗` failed
//! - the spinner frame       — active (pending / in progress)

use iocraft::prelude::*;

use crate::model::{ApplyModel, UnitState};
use crate::poll::GlyphState;

pub const CHECKMARK: &str = "✓";
pub const XMARK: &str = "✗";
pub const ROLLED_BACK: &str = "↩";
pub const UNCHANGED: &str = "·";
pub const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
// Log lines shown under the active unit — the tail of its ring, matching
// devenv-tui's "recent lines under the active node" rule.
const ACTIVE_LOG_LINES: usize = 5;

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

pub fn view(model: &ApplyModel) -> impl Into<AnyElement<'static>> {
    let mut rows: Vec<AnyElement<'static>> = Vec::new();
    for unit in &model.units {
        let path = unit.unit_path.join(" / ");
        let header = format!("{} {}", unit_mark(unit.state), path);
        rows.push(element!(Text(content: header)).into_any());
        for g in &unit.glyphs {
            let mut line = format!("  {} {}", glyph_mark(g.state), g.glyph_key);
            if let Some(ms) = g.next_retry_in_ms {
                // Server-computed countdown from the projection's
                // `next_retry_in_ms`; shown only while a retry is scheduled.
                line.push_str(&format!("  (retry in {ms}ms)"));
            }
            rows.push(element!(Text(content: line)).into_any());
        }
        if unit.state == UnitState::Active {
            let start = unit.logs.len().saturating_sub(ACTIVE_LOG_LINES);
            for log in unit.logs.iter().skip(start) {
                rows.push(element!(Text(content: format!("    {log}"))).into_any());
            }
        }
    }
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

// Self-animating components: a `Spinner` that advances its own frame on a
// timer, and a `StatusIndicator` that shows the spinner while active and a
// settled mark otherwise. They compile and are exported, but the run loop
// (`apply::run_tui`) does not yet mount them — it redraws `render_to_string`
// each poll, so the spinner frame only advances at poll cadence. Wiring the
// animated iocraft render loop is deferred polish (ADR 0033 §3); these are
// ready for it.
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
pub fn StatusIndicator(_hooks: Hooks, props: &StatusIndicatorProps) -> impl Into<AnyElement<'static>> {
    if props.active {
        element!(Spinner).into_any()
    } else {
        element!(Text(content: props.mark)).into_any()
    }
}
