//! The wire response the read-only path returns: the ordered dry run behind
//! `POST /plan` (ADR 0036), flat where [`report`](crate::report) is tree-shaped
//! because a plan is a sequence of ops, not a fold over outcomes. `action` reuses
//! [`GlyphAction`] so the apply and the plan name the same four verbs with one
//! definition. As in `report.rs` the `serde` tags are load-bearing: `golemctl
//! plan` parses these exact strings.
//!
//! `--against-host` (ADR 0058) adds a second, optional layer over the same
//! ops: each [`PlannedOp`] may carry an `observed` verdict, and the report as a
//! whole an aggregate [`Reality`]. Both are omitted, not merely `null`, when
//! the host was not asked, so a journal-only response stays byte-identical to
//! what it was before this layer existed.

use scroll_format::ContentId;
use serde::Serialize;
use std::collections::BTreeSet;

use crate::journal::GlyphOp;
use crate::observe::{Observation, Unknowable};
use crate::report::GlyphAction;

#[derive(Debug, Clone, Serialize)]
pub struct PlanReport {
    pub host: String,
    pub scroll_content_id: String,
    pub against_revision: Option<u64>,
    pub ops: Vec<PlannedOp>,
    pub reloads: Vec<PredictedReload>,
    pub summary: PlanSummary,
    /// Present only when `--against-host` asked for the host column. The
    /// absence — not a `null` — is what lets a client tell "not asked" from
    /// "asked, and it matched" (ADR 0058).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reality: Option<Reality>,
}

/// The wire spelling of [`Observation`](crate::observe::Observation): the same
/// four-valued verdict, minus `Unknowable`'s payload, which rides beside it in
/// [`PlannedOp::unobservable`] instead of being nested inside this tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Observed {
    Realized,
    Divergent,
    Absent,
    Unknown,
}

/// Why an op's [`Observed::Unknown`] couldn't be settled further. Present only
/// alongside `observed: unknown` — never on any other verdict (see
/// [`PlannedOp::observed_as`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Unobservable {
    Sealed,
    Unreadable,
    NotModelled,
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
    /// Omitted entirely on a journal-only plan (`PlannedOp::of` leaves it
    /// `None` and nothing calls `observed_as`), which is also what keeps that
    /// response byte-identical to before `--against-host` existed (ADR 0058).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed: Option<Observed>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unobservable: Option<Unobservable>,
}

/// Which way a predicted unit gets poked, mirroring the two derivations that
/// produce it: `Restart` from the ADR 0020 structural heuristic (a unit *file*
/// changed, only a true restart picks that up), `ReloadOrRestart` from an
/// authored `notifies` (ADR 0036). Where both name one unit, restart wins. The
/// kebab-case tag is the ADR's own spelling, `reload-or-restart`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReloadKind {
    Restart,
    ReloadOrRestart,
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

/// The host block's counts, over **distinct glyph keys**, not ops — a key
/// three units declare is counted once ([`Reality::over`]'s dedup). A
/// `Remove`'s resource never lands in `realized`/`divergent`/`absent`: those
/// three answer "does the host already have what's declared", which a remove
/// doesn't declare anything to have. It answers the weaker question instead —
/// is the resource gone yet — into `already_gone`/`still_present` (ADR 0058).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Reality {
    pub realized: usize,
    pub divergent: usize,
    pub absent: usize,
    pub unknown: usize,
    pub already_gone: usize,
    pub still_present: usize,
    /// Server-side judgement, not something a client should reconstruct by
    /// summing the fields above: an `unknown` must never count as agreement,
    /// and that is the rule such a computation would be likeliest to get
    /// wrong (ADR 0058).
    pub host_already_matches: bool,
}

impl PlannedOp {
    /// Which content ids an op carries falls out of the diff itself: an
    /// `Install` has no prior, a `Remove` no successor, a `Noop`'s two are equal
    /// by definition. They are rendered in `ContentId`'s hex `Display` form — the
    /// WAL stores the raw digest, hex is the shape a client can read and compare.
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
            observed: None,
            unobservable: None,
        }
    }

    pub fn observed_as(mut self, observation: Observation) -> Self {
        self.unobservable = match observation {
            Observation::Unknown(reason) => Some(match reason {
                Unknowable::Sealed => Unobservable::Sealed,
                Unknowable::Unreadable => Unobservable::Unreadable,
                Unknowable::NotModelled => Unobservable::NotModelled,
            }),
            _ => None,
        };
        self.observed = Some(match observation {
            Observation::Realized => Observed::Realized,
            Observation::Divergent => Observed::Divergent,
            Observation::Absent => Observed::Absent,
            Observation::Unknown(_) => Observed::Unknown,
        });
        self
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
                GlyphAction::Restart | GlyphAction::Reload => {}
            }
        }
        summary
    }
}

impl Reality {
    pub fn over(ops: &[PlannedOp]) -> Self {
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        let mut reality = Reality {
            realized: 0,
            divergent: 0,
            absent: 0,
            unknown: 0,
            already_gone: 0,
            still_present: 0,
            host_already_matches: false,
        };
        for op in ops {
            // NOTE: dedup by glyph key, not by op — a glyph declared by three
            // units still counts once (ADR 0058).
            if !seen.insert(op.glyph_key.as_str()) {
                continue;
            }
            let is_remove = op.action == GlyphAction::Remove;
            match (is_remove, op.observed) {
                (_, Some(Observed::Unknown)) => reality.unknown += 1,
                (true, Some(Observed::Absent)) => reality.already_gone += 1,
                (true, Some(_)) => reality.still_present += 1,
                (false, Some(Observed::Realized)) => reality.realized += 1,
                (false, Some(Observed::Divergent)) => reality.divergent += 1,
                (false, Some(Observed::Absent)) => reality.absent += 1,
                (_, None) => {}
            }
        }
        reality.host_already_matches = reality.divergent == 0
            && reality.absent == 0
            && reality.unknown == 0
            && reality.still_present == 0
            && (reality.realized + reality.already_gone) > 0;
        reality
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
    fn a_predicted_reload_serializes_its_kind_kebab_case() {
        let restart = PredictedReload {
            unit: "nginx.service".into(),
            kind: ReloadKind::Restart,
            triggered_by: vec!["file:/etc/systemd/system/nginx.service".into()],
        };
        let json = serde_json::to_value(&restart).unwrap();
        assert_eq!(json["kind"], "restart");
        assert_eq!(json["unit"], "nginx.service");

        let reload = PredictedReload {
            kind: ReloadKind::ReloadOrRestart,
            ..restart
        };
        assert_eq!(
            serde_json::to_value(&reload).unwrap()["kind"],
            "reload-or-restart"
        );
    }

    #[test]
    fn a_journal_only_plan_serializes_without_any_reality_fields() {
        let cid = content_id_of_glyph(&apt("x"));
        let op = PlannedOp::of(
            &unit_path(),
            &GlyphOp::Install {
                cid,
                glyph: apt("x"),
            },
        );
        let report = PlanReport {
            host: "host".to_string(),
            scroll_content_id: cid.to_string(),
            against_revision: Some(1),
            summary: PlanSummary::over(std::slice::from_ref(&op)),
            ops: vec![op],
            reloads: vec![],
            reality: None,
        };

        let json = serde_json::to_string(&report).unwrap();
        assert!(!json.contains("observed"));
        assert!(!json.contains("unobservable"));
        assert!(!json.contains("reality"));
    }

    #[test]
    fn an_observed_op_carries_its_verdict_and_no_reason() {
        let cid = content_id_of_glyph(&apt("x"));
        let op = PlannedOp::of(
            &unit_path(),
            &GlyphOp::Install {
                cid,
                glyph: apt("x"),
            },
        )
        .observed_as(Observation::Realized);

        let json = serde_json::to_value(&op).unwrap();
        assert_eq!(json["observed"], "realized");
        assert!(json.get("unobservable").is_none());
    }

    #[test]
    fn an_unknown_op_carries_both_the_verdict_and_the_reason() {
        let cid = content_id_of_glyph(&apt("x"));
        let op = PlannedOp::of(
            &unit_path(),
            &GlyphOp::Install {
                cid,
                glyph: apt("x"),
            },
        )
        .observed_as(Observation::Unknown(Unknowable::Sealed));

        let json = serde_json::to_value(&op).unwrap();
        assert_eq!(json["observed"], "unknown");
        assert_eq!(json["unobservable"], "sealed");
    }

    #[test]
    fn reality_serializes_snake_case_counts() {
        let reality = Reality {
            realized: 1,
            divergent: 2,
            absent: 3,
            unknown: 4,
            already_gone: 5,
            still_present: 6,
            host_already_matches: false,
        };

        let json = serde_json::to_value(reality).unwrap();
        assert_eq!(json["realized"], 1);
        assert_eq!(json["divergent"], 2);
        assert_eq!(json["absent"], 3);
        assert_eq!(json["unknown"], 4);
        assert_eq!(json["already_gone"], 5);
        assert_eq!(json["still_present"], 6);
        assert_eq!(json["host_already_matches"], false);
    }
}
