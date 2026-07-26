use std::collections::VecDeque;

use crate::poll::{GlyphState, Phase, Progress};

pub const LOG_RING_CAP: usize = 200;

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
    pub cursor: u64,
    pub report: Option<serde_json::Value>,
}

fn unit_state(glyphs: &[GlyphRow]) -> UnitState {
    if glyphs.iter().any(|g| g.state == GlyphState::Failed) {
        UnitState::Failed
    } else if glyphs.iter().all(|g| {
        matches!(
            g.state,
            GlyphState::Applied | GlyphState::Unchanged | GlyphState::RolledBack
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
            cursor: 0,
            report: None,
        }
    }

    pub fn apply_progress(&mut self, p: Progress) {
        self.reconcile_id = p.reconcile_id;
        self.phase = p.phase;
        self.cursor = p.cursor;
        self.report = p.report;
        for up in p.units {
            let glyphs: Vec<GlyphRow> = up
                .glyphs
                .into_iter()
                .map(|g| GlyphRow {
                    glyph_key: g.glyph_key,
                    action: g.action,
                    state: g.state,
                    rounds: g.rounds,
                    next_retry_in_ms: g.next_retry_in_ms,
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
            if let Some(node) = self.units.iter_mut().find(|u| u.unit_path == ev.unit_path) {
                node.logs.push_back(format!("{}: {}", ev.glyph_key, ev.message));
                while node.logs.len() > LOG_RING_CAP {
                    node.logs.pop_front();
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
