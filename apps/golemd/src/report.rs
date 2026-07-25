//! The wire report the write path returns, tree-shaped to mirror the host scroll
//! (ADR 0029 §5): a top-level roll-up over one [`UnitReport`] per leaf unit, each
//! carrying that unit's failed glyphs. `apply_manifest` returns this for every
//! reconcile — a partial or rolled-back unit is reported *in-band*, not as an HTTP
//! error (see `http.rs`). The `serde` tags here are load-bearing: the fleet CLI
//! parses these exact strings, so a rename is a client-visible change, not a
//! refactor.

use serde::Serialize;

use crate::journal::Revision;

/// The reconcile's rolled-up fate across all units (`roll_up`). Serializes
/// snake_case (`settled` | `partial` | `rolled_back`) — the tags `fleet apply`
/// colors the whole apply by.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TopOutcome {
    Settled,
    Partial,
    RolledBack,
}

/// One leaf unit's fate: `Settled` (no failures), `RolledBack` (its
/// `on_exhaust = rollback` undid this attempt's glyphs), or `Partial` (its
/// `on_exhaust = keep` left the applied ones committed). Same snake_case tags as
/// [`TopOutcome`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitOutcome {
    Settled,
    Partial,
    RolledBack,
}

/// Which side of the bracket a glyph failed on. `Recovery` is reserved for a
/// failure surfaced by crash recovery; the live enact loop emits only `Enact`
/// and `Reverse`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailPhase {
    Enact,
    Reverse,
    Recovery,
}

/// Why a glyph gave up: `Fatal` (never retried) or `RetriesExhausted` (a
/// retryable that hit a limit). Serialized `fatal` / `retries-exhausted` (kebab,
/// not the enum's default) — the exact tags the fleet CLI matches on.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum FailClassReport {
    #[serde(rename = "fatal")]
    Fatal,
    #[serde(rename = "retries-exhausted")]
    RetriesExhausted,
}

/// One glyph a unit could not settle. `attempts` is the rounds it ran;
/// `rolled_back` is `true` only when its unit's `on_exhaust = rollback` undid it.
/// `message` is the reconciler's reason — glyph key and reason only, never
/// contents or secrets.
#[derive(Debug, Clone, Serialize)]
pub struct GlyphFailure {
    pub glyph_key: String,
    pub unit_path: Vec<String>,
    pub phase: FailPhase,
    pub class: FailClassReport,
    pub attempts: u32,
    pub message: String,
    pub rolled_back: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GlyphAction {
    Install,
    Replace,
    Remove,
    Noop,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GlyphOutcome {
    Applied,
    Unchanged,
    Failed,
    RolledBack,
}

#[derive(Debug, Clone, Serialize)]
pub struct GlyphLine {
    pub glyph_key: String,
    pub action: GlyphAction,
    pub outcome: GlyphOutcome,
    pub attempts: u32,
    pub message: Option<String>,
}

/// One leaf unit's slice of the report: its root-to-leaf `unit_path`, its
/// outcome, the per-glyph lines in enact order, and the glyphs it left failing.
#[derive(Debug, Clone, Serialize)]
pub struct UnitReport {
    pub unit_path: Vec<String>,
    pub outcome: UnitOutcome,
    pub glyphs: Vec<GlyphLine>,
    pub failures: Vec<GlyphFailure>,
}

/// The write path's return value: the `Revision` this attempt committed, the
/// rolled-up `outcome`, and the per-unit reports in source order.
#[derive(Debug, Clone, Serialize)]
pub struct ReconcileReport {
    pub revision: Revision,
    pub outcome: TopOutcome,
    pub units: Vec<UnitReport>,
}

impl ReconcileReport {
    /// Roll the per-unit outcomes into the top-level one (ADR 0029 §5): `Settled`
    /// only when every unit settled; otherwise `Partial` if any unit kept a
    /// partial set, else `RolledBack`.
    pub fn roll_up(revision: Revision, units: Vec<UnitReport>) -> ReconcileReport {
        let outcome = if units.iter().all(|u| u.outcome == UnitOutcome::Settled) {
            TopOutcome::Settled
        } else if units.iter().any(|u| u.outcome == UnitOutcome::Partial) {
            TopOutcome::Partial
        } else {
            TopOutcome::RolledBack
        };
        ReconcileReport {
            revision,
            outcome,
            units,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_settled_unit_serializes_with_snake_case_outcome() {
        let unit = UnitReport {
            unit_path: vec!["h".into(), "u".into()],
            outcome: UnitOutcome::Settled,
            glyphs: vec![],
            failures: vec![],
        };
        let json = serde_json::to_value(&unit).unwrap();
        assert_eq!(json["outcome"], "settled");
    }

    #[test]
    fn glyph_line_actions_and_outcomes_serialize_snake_case() {
        let applied = GlyphLine {
            glyph_key: "apt:podman".into(),
            action: GlyphAction::Install,
            outcome: GlyphOutcome::Applied,
            attempts: 1,
            message: None,
        };
        let unchanged = GlyphLine {
            glyph_key: "file:/x".into(),
            action: GlyphAction::Noop,
            outcome: GlyphOutcome::Unchanged,
            attempts: 0,
            message: None,
        };
        let failed = GlyphLine {
            glyph_key: "systemd:fishnet.service".into(),
            action: GlyphAction::Replace,
            outcome: GlyphOutcome::Failed,
            attempts: 5,
            message: Some("mirror down".into()),
        };
        let rolled_back = GlyphLine {
            glyph_key: "apt:podman".into(),
            action: GlyphAction::Remove,
            outcome: GlyphOutcome::RolledBack,
            attempts: 1,
            message: None,
        };
        assert_eq!(serde_json::to_value(&applied).unwrap()["action"], "install");
        assert_eq!(
            serde_json::to_value(&applied).unwrap()["outcome"],
            "applied"
        );
        assert_eq!(
            serde_json::to_value(&unchanged).unwrap()["outcome"],
            "unchanged"
        );
        assert_eq!(serde_json::to_value(&unchanged).unwrap()["action"], "noop");
        assert_eq!(serde_json::to_value(&failed).unwrap()["outcome"], "failed");
        assert_eq!(
            serde_json::to_value(&failed).unwrap()["action"],
            "replace"
        );
        assert_eq!(
            serde_json::to_value(&rolled_back).unwrap()["outcome"],
            "rolled_back"
        );
        assert_eq!(
            serde_json::to_value(&rolled_back).unwrap()["action"],
            "remove"
        );
    }

    #[test]
    fn a_glyph_failure_class_renders_retries_exhausted() {
        let f = GlyphFailure {
            glyph_key: "apt:x".into(),
            unit_path: vec!["h".into()],
            phase: FailPhase::Enact,
            class: FailClassReport::RetriesExhausted,
            attempts: 3,
            message: "mirror down".into(),
            rolled_back: false,
        };
        let json = serde_json::to_value(&f).unwrap();
        assert_eq!(json["class"], "retries-exhausted");
        assert_eq!(json["phase"], "enact");
    }

    #[test]
    fn roll_up_is_settled_only_when_all_units_settle() {
        let rev = crate::journal::Revision {
            id: 2,
            created_at: chrono::Utc::now(),
            kind: crate::journal::RevisionKind::Reconcile,
            scroll_content_id: None,
            outcomes: vec![],
        };
        let settled = UnitReport {
            unit_path: vec!["h".into(), "a".into()],
            outcome: UnitOutcome::Settled,
            glyphs: vec![],
            failures: vec![],
        };
        let partial = UnitReport {
            unit_path: vec!["h".into(), "b".into()],
            outcome: UnitOutcome::Partial,
            glyphs: vec![],
            failures: vec![],
        };
        let all_settled = ReconcileReport::roll_up(rev.clone(), vec![settled.clone()]);
        assert_eq!(all_settled.outcome, TopOutcome::Settled);
        let mixed = ReconcileReport::roll_up(rev, vec![settled, partial]);
        assert_eq!(mixed.outcome, TopOutcome::Partial);
    }
}
