//! Shared types between `golemd` and `golemctl`.
//!
//! Three nouns:
//!
//! - [`Blueprint`] — what a user commissions. A name + a list of
//!   packages they want present on the node.
//! - [`State`] — the canonical resolved view across all currently
//!   commissioned blueprints: each package mapped to the blueprints
//!   that asked for it.
//! - [`Revision`] — a historical entry in the node's journal. Every
//!   commission / decommission produces one. Each revision embeds the
//!   resolved [`State`] at that moment plus the [`Action`]s the agent
//!   would take to transition from the previous revision to this one.
//!
//! The verbs: a user **commissions** a blueprint (requesting it be
//! present) or **decommissions** it (requesting it be gone). What golem
//! does in answer to a successful commission / decommission is **build**
//! / **teardown** — the realization phase. Today nothing is realized;
//! the [`Action`]s (`Install` / `Remove`) that a build / teardown would
//! comprise are only recorded. No installation, no signing, no
//! providers — this is a bookkeeping agent. Adding real-world
//! enforcement happens layered on top of this, not in it.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// What a user commissions.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Blueprint {
    pub name: String,
    #[serde(default)]
    pub packages: Vec<String>,
}

/// Canonical resolved state. `packages[name]` is the set of blueprint
/// names that currently want `name` present. Empty `packages` means no
/// blueprint is asking for anything (e.g. fresh node, or every
/// blueprint has been decommissioned).
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct State {
    pub packages: BTreeMap<String, BTreeSet<String>>,
}

impl State {
    /// Recompute state from the full set of currently commissioned
    /// blueprints.
    pub fn resolve(blueprints: &BTreeMap<String, Blueprint>) -> Self {
        let mut packages: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for bp in blueprints.values() {
            for pkg in &bp.packages {
                packages
                    .entry(pkg.clone())
                    .or_default()
                    .insert(bp.name.clone());
            }
        }
        Self { packages }
    }

    /// Actions that would transition `from` → `self`. An `Install`
    /// when a package newly appears; a `Remove` when it newly
    /// disappears. Changes only to *which blueprints* want a package
    /// (without the package itself entering or leaving) do not produce
    /// actions.
    pub fn actions_from(&self, prior: &State) -> Vec<Action> {
        let prior_keys: BTreeSet<&String> = prior.packages.keys().collect();
        let new_keys: BTreeSet<&String> = self.packages.keys().collect();
        let mut actions = Vec::new();
        for pkg in new_keys.difference(&prior_keys) {
            actions.push(Action::Install {
                package: (*pkg).clone(),
            });
        }
        for pkg in prior_keys.difference(&new_keys) {
            actions.push(Action::Remove {
                package: (*pkg).clone(),
            });
        }
        actions
    }
}

/// A bookkeeping record of one step a build / teardown *would* take at
/// a transition. No execution is implied today.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    Install { package: String },
    Remove { package: String },
}

/// What kind of transition produced this revision.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RevisionKind {
    /// The node's initial empty revision, written on first boot.
    Init,
    /// A blueprint was commissioned (new or replacing an existing one
    /// of the same name).
    Commission,
    /// A blueprint was decommissioned by name.
    Decommission,
}

/// One entry in the node's journal.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Revision {
    pub id: u64,
    pub at: DateTime<Utc>,
    pub kind: RevisionKind,
    /// The blueprint name involved, when applicable. `None` for
    /// [`RevisionKind::Init`].
    pub blueprint: Option<String>,
    /// Transition actions from the previous revision to this one.
    pub actions: Vec<Action>,
    /// The full resolved state as of this revision. Embedded so
    /// querying `/revisions/:id` returns everything you need without
    /// reconstructing.
    pub state: State,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blueprint(name: &str, pkgs: &[&str]) -> Blueprint {
        Blueprint {
            name: name.into(),
            packages: pkgs.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn resolve_unions_packages_and_tracks_owners() {
        let mut active = BTreeMap::new();
        active.insert("web".into(), blueprint("web", &["nginx", "curl"]));
        active.insert(
            "monitoring".into(),
            blueprint("monitoring", &["curl", "prometheus-node-exporter"]),
        );
        let st = State::resolve(&active);

        assert_eq!(
            st.packages["nginx"],
            BTreeSet::from(["web".to_string()])
        );
        assert_eq!(
            st.packages["curl"],
            BTreeSet::from(["web".to_string(), "monitoring".to_string()])
        );
        assert_eq!(
            st.packages["prometheus-node-exporter"],
            BTreeSet::from(["monitoring".to_string()])
        );
    }

    #[test]
    fn decommissioning_one_blueprint_keeps_shared_packages() {
        let mut active = BTreeMap::new();
        active.insert("web".into(), blueprint("web", &["nginx", "curl"]));
        active.insert("monitoring".into(), blueprint("monitoring", &["curl", "prom"]));
        let before = State::resolve(&active);

        active.remove("web");
        let after = State::resolve(&active);

        let actions = after.actions_from(&before);
        // nginx is gone; curl stays (monitoring still wants it); prom stays.
        assert_eq!(
            actions,
            vec![Action::Remove {
                package: "nginx".into(),
            }]
        );
        assert!(after.packages.contains_key("curl"));
        assert!(after.packages.contains_key("prom"));
    }

    #[test]
    fn actions_only_fire_when_set_membership_changes() {
        // Two blueprints both wanting curl. Drop one; curl stays.
        let mut active = BTreeMap::new();
        active.insert("a".into(), blueprint("a", &["curl"]));
        active.insert("b".into(), blueprint("b", &["curl"]));
        let before = State::resolve(&active);

        active.remove("a");
        let after = State::resolve(&active);

        let actions = after.actions_from(&before);
        assert!(actions.is_empty(), "expected no actions, got {:?}", actions);
    }
}
