use serde::Serialize;

use crate::journal::Revision;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TopOutcome {
    Settled,
    Partial,
    RolledBack,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitOutcome {
    Settled,
    Partial,
    RolledBack,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailPhase {
    Enact,
    Reverse,
    Recovery,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum FailClassReport {
    #[serde(rename = "fatal")]
    Fatal,
    #[serde(rename = "retries-exhausted")]
    RetriesExhausted,
}

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

#[derive(Debug, Clone, Serialize)]
pub struct UnitReport {
    pub unit_path: Vec<String>,
    pub outcome: UnitOutcome,
    pub failures: Vec<GlyphFailure>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReconcileReport {
    pub revision: Revision,
    pub outcome: TopOutcome,
    pub units: Vec<UnitReport>,
}

impl ReconcileReport {
    pub fn roll_up(revision: Revision, units: Vec<UnitReport>) -> ReconcileReport {
        let outcome = if units.iter().all(|u| u.outcome == UnitOutcome::Settled) {
            TopOutcome::Settled
        } else if units.iter().any(|u| u.outcome == UnitOutcome::Partial) {
            TopOutcome::Partial
        } else {
            TopOutcome::RolledBack
        };
        ReconcileReport { revision, outcome, units }
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
            failures: vec![],
        };
        let json = serde_json::to_value(&unit).unwrap();
        assert_eq!(json["outcome"], "settled");
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
            failures: vec![],
        };
        let partial = UnitReport {
            unit_path: vec!["h".into(), "b".into()],
            outcome: UnitOutcome::Partial,
            failures: vec![],
        };
        let all_settled = ReconcileReport::roll_up(rev.clone(), vec![settled.clone()]);
        assert_eq!(all_settled.outcome, TopOutcome::Settled);
        let mixed = ReconcileReport::roll_up(rev, vec![settled, partial]);
        assert_eq!(mixed.outcome, TopOutcome::Partial);
    }
}
