use scroll_format::ContentId;
use serde::Serialize;

use crate::journal::GlyphOp;
use crate::report::GlyphAction;

#[derive(Debug, Clone, Serialize)]
pub struct PlanReport {
    pub host: String,
    pub scroll_content_id: String,
    pub against_revision: Option<u64>,
    pub ops: Vec<PlannedOp>,
    pub reloads: Vec<PredictedReload>,
    pub summary: PlanSummary,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlannedOp {
    pub unit_path: Vec<String>,
    pub glyph_key: String,
    pub action: GlyphAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_cid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_cid: Option<String>,
    pub describe: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReloadKind {
    Restart,
}

#[derive(Debug, Clone, Serialize)]
pub struct PredictedReload {
    pub unit: String,
    pub kind: ReloadKind,
    pub triggered_by: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct PlanSummary {
    pub install: usize,
    pub replace: usize,
    pub remove: usize,
    pub noop: usize,
}

impl PlannedOp {
    pub fn of(unit_path: &[String], op: &GlyphOp) -> Self {
        let (action, old_cid, new_cid) = match op {
            GlyphOp::Install { cid, .. } => (GlyphAction::Install, None, Some(hex(cid))),
            GlyphOp::Remove { cid, .. } => (GlyphAction::Remove, Some(hex(cid)), None),
            GlyphOp::Replace {
                old_cid, new_cid, ..
            } => (GlyphAction::Replace, Some(hex(old_cid)), Some(hex(new_cid))),
            GlyphOp::Noop { cid, .. } => (GlyphAction::Noop, Some(hex(cid)), Some(hex(cid))),
        };
        PlannedOp {
            unit_path: unit_path.to_vec(),
            glyph_key: op.key(),
            action,
            old_cid,
            new_cid,
            describe: op.glyph().describe(),
        }
    }
}

impl PlanSummary {
    pub fn over(ops: &[PlannedOp]) -> Self {
        let mut summary = PlanSummary::default();
        for op in ops {
            match op.action {
                GlyphAction::Install => summary.install += 1,
                GlyphAction::Replace => summary.replace += 1,
                GlyphAction::Remove => summary.remove += 1,
                GlyphAction::Noop => summary.noop += 1,
            }
        }
        summary
    }
}

fn hex(cid: &ContentId) -> String {
    cid.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use scroll_format::{content_id_of_glyph, Glyph};

    fn apt(name: &str) -> Glyph {
        Glyph::AptPackage { name: name.into() }
    }

    fn unit_path() -> Vec<String> {
        vec!["host".to_string(), "web".to_string()]
    }

    #[test]
    fn an_install_carries_only_a_hex_new_cid() {
        let glyph = apt("nginx");
        let cid = content_id_of_glyph(&glyph);
        let op = GlyphOp::Install {
            cid,
            glyph: glyph.clone(),
        };
        let planned = PlannedOp::of(&unit_path(), &op);
        let json = serde_json::to_value(&planned).unwrap();
        assert_eq!(json["action"], "install");
        assert_eq!(json["glyph_key"], "apt:nginx");
        assert_eq!(json["new_cid"], cid.to_string());
        assert!(json.get("old_cid").is_none());
        assert_eq!(json["unit_path"], serde_json::json!(["host", "web"]));
        assert_eq!(json["describe"], glyph.describe());
    }

    #[test]
    fn a_replace_carries_both_cids_and_a_remove_only_the_old_one() {
        let old = content_id_of_glyph(&apt("old"));
        let new = content_id_of_glyph(&apt("new"));
        let replace = PlannedOp::of(
            &unit_path(),
            &GlyphOp::Replace {
                old_cid: old,
                new_cid: new,
                glyph: apt("new"),
            },
        );
        assert_eq!(replace.old_cid, Some(old.to_string()));
        assert_eq!(replace.new_cid, Some(new.to_string()));

        let remove = PlannedOp::of(
            &unit_path(),
            &GlyphOp::Remove {
                cid: old,
                glyph: apt("old"),
            },
        );
        assert_eq!(remove.old_cid, Some(old.to_string()));
        assert_eq!(remove.new_cid, None);
    }

    #[test]
    fn the_summary_counts_every_action() {
        let cid = content_id_of_glyph(&apt("x"));
        let ops = vec![
            PlannedOp::of(
                &unit_path(),
                &GlyphOp::Install {
                    cid,
                    glyph: apt("a"),
                },
            ),
            PlannedOp::of(
                &unit_path(),
                &GlyphOp::Noop {
                    cid,
                    glyph: apt("b"),
                },
            ),
            PlannedOp::of(
                &unit_path(),
                &GlyphOp::Noop {
                    cid,
                    glyph: apt("c"),
                },
            ),
            PlannedOp::of(
                &unit_path(),
                &GlyphOp::Remove {
                    cid,
                    glyph: apt("d"),
                },
            ),
        ];
        assert_eq!(
            PlanSummary::over(&ops),
            PlanSummary {
                install: 1,
                replace: 0,
                remove: 1,
                noop: 2,
            }
        );
    }

    #[test]
    fn a_predicted_reload_serializes_its_kind_snake_case() {
        let reload = PredictedReload {
            unit: "nginx.service".into(),
            kind: ReloadKind::Restart,
            triggered_by: vec!["file:/etc/systemd/system/nginx.service".into()],
        };
        let json = serde_json::to_value(&reload).unwrap();
        assert_eq!(json["kind"], "restart");
        assert_eq!(json["unit"], "nginx.service");
    }
}
