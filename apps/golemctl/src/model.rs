//! The apply model and the fold that drives it from poll responses (ADR 0033
//! §3). The model/fold/view split is adopted from devenv-tui's
//! `ActivityModel`: a tree of nodes, typed events folded into it, and a view
//! that is a pure function of the model — `iocraft` is the shared dependency
//! underneath. Here the tree is one [`UnitNode`] per `unit_path`, and the poll
//! response replaces devenv's activity-event channel as the fold's input.
//!
//! [`apply_progress`](ApplyModel::apply_progress) upserts each unit's glyph
//! rows by path (the projection is authoritative — a later poll overwrites the
//! prior state), then appends the poll's `events` to the matching node's log
//! ring. Nodes are never removed; the projection only grows.

use std::collections::VecDeque;
use std::path::PathBuf;

use crate::poll::{EventKind, GlyphState, Phase, Progress};

// Per-unit log lines retained; the view shows only the last few of an active
// unit, so the ring bounds memory without touching what the user sees.
pub const LOG_RING_CAP: usize = 200;

// The buildkit-style tail (ADR 0033 §3d): the last few `kind:"cmd"` lines shown
// dim and indented under an active glyph row, rolling as new lines arrive. Three
// is the on-screen window, not the retention — the log files (§3a) keep them all.
pub const CMD_TAIL_LINES: usize = 3;

// NOTE: superseded by `tree::BranchState` (ADR 0033 §3c). The renderer builds a
// real tree and aggregates branch state from the glyph rows directly, so it
// never reads `UnitNode.state`. This flat per-unit rollup and `unit_state`
// survive only because `model_tests` still assert on them; nothing in the render
// path does. Retire both if those tests move to asserting on the tree.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnitState {
    Active,
    Settled,
    Failed,
}

#[derive(Debug, Clone)]
pub struct GlyphRow {
    pub glyph_key: String,
    pub action: String,
    pub state: GlyphState,
    pub rounds: u32,
    pub next_retry_in_ms: Option<u64>,
    // A shared duplicate (ADR 0034 §1): an earlier unit enacts this `(key, cid)`,
    // so the row is context, not work — a slow dim spinner while waiting, then the
    // `≡` credit mark. `owner` is the first declarer's unit_path, named in the
    // row's suffix.
    pub shared: bool,
    pub owner: Option<Vec<String>>,
    // The rolling tail of this glyph's `kind:"cmd"` lines (ADR 0033 §3d), bounded
    // at CMD_TAIL_LINES and cleared when the glyph settles so the tree stays
    // compact.
    pub cmd_tail: VecDeque<String>,
}

#[derive(Debug, Clone)]
pub struct UnitNode {
    pub unit_path: Vec<String>,
    pub glyphs: Vec<GlyphRow>,
    pub logs: VecDeque<String>,
    pub state: UnitState,
}

#[derive(Debug, Clone)]
pub struct ApplyModel {
    pub reconcile_id: u64,
    pub phase: Phase,
    pub units: Vec<UnitNode>,
    pub root_logs: VecDeque<String>,
    pub cursor: u64,
    pub report: Option<serde_json::Value>,
    pub log_dir: Option<PathBuf>,
}

// A glyph is terminal once it has settled to any non-in-flight state (ADR 0033
// §3d) — the point its rolling cmd tail collapses.
fn is_terminal(state: GlyphState) -> bool {
    !matches!(state, GlyphState::Pending | GlyphState::InProgress)
}

// A unit is Failed if any glyph failed, Settled only once every glyph reached a
// terminal non-failed state (applied/unchanged/rolled-back), else Active. Failed
// wins over a still-pending sibling so a failure is never hidden by in-flight work.
fn unit_state(glyphs: &[GlyphRow]) -> UnitState {
    if glyphs.iter().any(|g| g.state == GlyphState::Failed) {
        UnitState::Failed
    } else if glyphs.iter().all(|g| {
        matches!(
            g.state,
            GlyphState::Applied
                | GlyphState::Unchanged
                | GlyphState::RolledBack
                | GlyphState::Credited
        )
    }) {
        UnitState::Settled
    } else {
        UnitState::Active
    }
}

impl ApplyModel {
    pub fn new() -> Self {
        Self {
            reconcile_id: 0,
            phase: Phase::Planning,
            units: Vec::new(),
            root_logs: VecDeque::new(),
            cursor: 0,
            report: None,
            log_dir: None,
        }
    }

    pub fn apply_progress(&mut self, p: Progress) {
        self.reconcile_id = p.reconcile_id;
        self.phase = p.phase;
        self.cursor = p.cursor;
        self.report = p.report;
        for up in p.units {
            let existing = self.units.iter().find(|u| u.unit_path == up.unit_path);
            let glyphs: Vec<GlyphRow> = up
                .glyphs
                .into_iter()
                .map(|g| {
                    // The projection is authoritative for state, but the cmd tail
                    // (§3d) accrues client-side across polls, so carry it forward
                    // by key — then drop it once the glyph settles.
                    let cmd_tail = if is_terminal(g.state) {
                        VecDeque::new()
                    } else {
                        existing
                            .and_then(|n| n.glyphs.iter().find(|r| r.glyph_key == g.glyph_key))
                            .map(|r| r.cmd_tail.clone())
                            .unwrap_or_default()
                    };
                    GlyphRow {
                        glyph_key: g.glyph_key,
                        action: g.action,
                        state: g.state,
                        rounds: g.rounds,
                        next_retry_in_ms: g.next_retry_in_ms,
                        shared: g.shared,
                        owner: g.owner,
                        cmd_tail,
                    }
                })
                .collect();
            let state = unit_state(&glyphs);
            match self.units.iter_mut().find(|u| u.unit_path == up.unit_path) {
                Some(node) => {
                    node.glyphs = glyphs;
                    node.state = state;
                }
                None => self.units.push(UnitNode {
                    unit_path: up.unit_path,
                    glyphs,
                    logs: VecDeque::new(),
                    state,
                }),
            }
        }
        for ev in p.events {
            if ev.kind == EventKind::Cmd {
                if let Some(row) = self
                    .units
                    .iter_mut()
                    .find(|u| u.unit_path == ev.unit_path)
                    .and_then(|n| n.glyphs.iter_mut().find(|r| r.glyph_key == ev.glyph_key))
                {
                    if !is_terminal(row.state) {
                        row.cmd_tail.push_back(ev.message);
                        while row.cmd_tail.len() > CMD_TAIL_LINES {
                            row.cmd_tail.pop_front();
                        }
                    }
                }
                continue;
            }
            let line = format!("{}: {}", ev.glyph_key, ev.message);
            match self.units.iter_mut().find(|u| u.unit_path == ev.unit_path) {
                Some(node) => {
                    node.logs.push_back(line);
                    while node.logs.len() > LOG_RING_CAP {
                        node.logs.pop_front();
                    }
                }
                None => {
                    self.root_logs.push_back(line);
                    while self.root_logs.len() > LOG_RING_CAP {
                        self.root_logs.pop_front();
                    }
                }
            }
        }
    }

    pub fn is_settled(&self) -> bool {
        self.phase.is_terminal()
    }
}

impl Default for ApplyModel {
    fn default() -> Self {
        Self::new()
    }
}
