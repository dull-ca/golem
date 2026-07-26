//! The render tree over the fold (ADR 0033 §3c). The model keeps a flat
//! `Vec<UnitNode>` keyed by `unit_path`; this rebuilds it as a real tree that
//! mirrors the recursive scroll — one [`TreeNode`] per path prefix, leaf units
//! carrying their glyph rows and branch nodes aggregating the state of their
//! subtree. Building it here, as a pure function of the units, keeps the fold
//! simple and puts the tree shape under `render_to_string`'s tested surface.

use crate::model::UnitNode;
use crate::poll::GlyphState;

// The one status vocabulary the whole tree speaks (ADR 0033 §3c). Leaf glyphs
// fold into it; a branch AGGREGATES its subtree into the same set, so a spinner
// or a mark reads identically at any depth.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BranchState {
    Active,
    Failed,
    RolledBack,
    Applied,
    Unchanged,
}

pub struct TreeNode<'a> {
    pub segment: String,
    pub path: Vec<String>,
    pub state: BranchState,
    pub leaf: Option<&'a UnitNode>,
    pub children: Vec<TreeNode<'a>>,
}

pub fn build(units: &[UnitNode]) -> Vec<TreeNode<'_>> {
    let mut roots: Vec<TreeNode> = Vec::new();
    for unit in units {
        insert(&mut roots, unit, 0);
    }
    for root in &mut roots {
        resolve(root);
    }
    roots
}

fn insert<'a>(level: &mut Vec<TreeNode<'a>>, unit: &'a UnitNode, depth: usize) {
    let segment = unit.unit_path[depth].clone();
    let idx = match level.iter().position(|n| n.segment == segment) {
        Some(i) => i,
        None => {
            level.push(TreeNode {
                segment: segment.clone(),
                path: unit.unit_path[..=depth].to_vec(),
                state: BranchState::Active,
                leaf: None,
                children: Vec::new(),
            });
            level.len() - 1
        }
    };
    if depth + 1 == unit.unit_path.len() {
        level[idx].leaf = Some(unit);
    } else {
        insert(&mut level[idx].children, unit, depth + 1);
    }
}

// Post-order: a branch's state is the aggregate of its own leaf glyphs plus
// every child subtree, in the precedence order of ADR 0033 §3c — active if
// anything is still moving, else failed, else rolled_back, else settled
// (applied if any work happened, unchanged if every descendant was a Noop).
fn resolve(node: &mut TreeNode) {
    for child in &mut node.children {
        resolve(child);
    }
    node.state = aggregate(node);
}

fn aggregate(node: &TreeNode) -> BranchState {
    let mut any_active = false;
    let mut any_failed = false;
    let mut any_rolled_back = false;
    let mut any_applied = false;

    if let Some(unit) = node.leaf {
        for g in &unit.glyphs {
            match g.state {
                GlyphState::Pending | GlyphState::InProgress => any_active = true,
                GlyphState::Failed => any_failed = true,
                GlyphState::RolledBack => any_rolled_back = true,
                GlyphState::Applied => any_applied = true,
                GlyphState::Unchanged => {}
            }
        }
    }
    for child in &node.children {
        match child.state {
            BranchState::Active => any_active = true,
            BranchState::Failed => any_failed = true,
            BranchState::RolledBack => any_rolled_back = true,
            BranchState::Applied => any_applied = true,
            BranchState::Unchanged => {}
        }
    }

    if any_active {
        BranchState::Active
    } else if any_failed {
        BranchState::Failed
    } else if any_rolled_back {
        BranchState::RolledBack
    } else if any_applied {
        BranchState::Applied
    } else {
        BranchState::Unchanged
    }
}

impl BranchState {
    pub fn is_active(self) -> bool {
        self == BranchState::Active
    }

    pub fn is_settled(self) -> bool {
        matches!(
            self,
            BranchState::Applied | BranchState::Unchanged | BranchState::RolledBack
        )
    }
}
