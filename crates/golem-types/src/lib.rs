//! Shared model types for golem. See `TERMINOLOGY.md`.
//!
//! A [`Blueprint`] is a set of [`Host`]s; each Host carries the [`Workload`]s,
//! [`Service`]s, and [`Ingress`] placed on it. [`State`] is the resolved view
//! across every commissioned Blueprint; diffing two States yields the
//! [`Action`]s a build/teardown comprises; each change is journalled as a
//! [`Revision`].

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// A named, self-contained system: a set of Hosts and what runs on them.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Blueprint {
    pub name: String,
    #[serde(default)]
    pub hosts: Vec<Host>,
}

/// A machine, and the container for everything that runs on it.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Host {
    pub name: String,
    #[serde(default)]
    pub workloads: Vec<Workload>,
    #[serde(default)]
    pub services: Vec<Service>,
    #[serde(default)]
    pub ingress: Vec<Ingress>,
}

/// A container that runs but is not attached to any network.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Workload {
    pub name: String,
    #[serde(default)]
    pub image: String,
}

/// A container on the blueprint-internal network.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Service {
    pub name: String,
    #[serde(default)]
    pub image: String,
    #[serde(default)]
    pub port: u16,
}

/// How traffic is allowed into the blueprint.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Ingress {
    pub name: String,
    #[serde(default)]
    pub service: String,
    #[serde(default)]
    pub from: IngressFrom,
    #[serde(default)]
    pub port: u16,
}

/// Where ingress traffic is allowed to originate.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IngressFrom {
    #[default]
    World,
    Internal,
}

/// The resolved view across every commissioned Blueprint: per host, each item
/// name maps to the set of Blueprints that call for it. An item stays as long
/// as at least one Blueprint wants it.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct State {
    pub hosts: BTreeMap<String, HostState>,
}

/// One host's slice of [`State`]: per item kind, each item name maps to the
/// set of Blueprint names that call for it.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct HostState {
    pub workloads: BTreeMap<String, BTreeSet<String>>,
    pub services: BTreeMap<String, BTreeSet<String>>,
    pub ingress: BTreeMap<String, BTreeSet<String>>,
}

impl State {
    /// Recompute the resolved state from the full set of commissioned
    /// Blueprints.
    pub fn resolve(blueprints: &BTreeMap<String, Blueprint>) -> Self {
        let mut state = State::default();
        for bp in blueprints.values() {
            for host in &bp.hosts {
                let hs = state.hosts.entry(host.name.clone()).or_default();
                for w in &host.workloads {
                    hs.workloads.entry(w.name.clone()).or_default().insert(bp.name.clone());
                }
                for s in &host.services {
                    hs.services.entry(s.name.clone()).or_default().insert(bp.name.clone());
                }
                for i in &host.ingress {
                    hs.ingress.entry(i.name.clone()).or_default().insert(bp.name.clone());
                }
            }
        }
        state
    }

    /// The Actions that move `prior` → `self`: a build when an item newly
    /// appears, a teardown when it newly disappears. A change only in *which*
    /// Blueprints want an item produces no Action.
    pub fn actions_from(&self, prior: &State) -> Vec<Action> {
        let mut actions = Vec::new();
        let empty = HostState::default();
        let hosts: BTreeSet<&String> = self.hosts.keys().chain(prior.hosts.keys()).collect();
        for host in hosts {
            let now = self.hosts.get(host).unwrap_or(&empty);
            let was = prior.hosts.get(host).unwrap_or(&empty);
            actions.extend(actions_for_kind(host, &was.workloads, &now.workloads, ItemKind::Workload));
            actions.extend(actions_for_kind(host, &was.services, &now.services, ItemKind::Service));
            actions.extend(actions_for_kind(host, &was.ingress, &now.ingress, ItemKind::Ingress));
        }
        actions
    }
}

#[derive(Clone, Copy)]
enum ItemKind {
    Workload,
    Service,
    Ingress,
}

fn actions_for_kind(
    host: &str,
    was: &BTreeMap<String, BTreeSet<String>>,
    now: &BTreeMap<String, BTreeSet<String>>,
    kind: ItemKind,
) -> Vec<Action> {
    let mut actions = Vec::new();
    for name in now.keys() {
        if !was.contains_key(name) {
            actions.push(Action::build(kind, host, name));
        }
    }
    for name in was.keys() {
        if !now.contains_key(name) {
            actions.push(Action::teardown(kind, host, name));
        }
    }
    actions
}

/// One recorded step within a build/teardown.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum Action {
    BuildWorkload { host: String, name: String },
    TeardownWorkload { host: String, name: String },
    BuildService { host: String, name: String },
    TeardownService { host: String, name: String },
    BuildIngress { host: String, name: String },
    TeardownIngress { host: String, name: String },
}

impl Action {
    fn build(kind: ItemKind, host: &str, name: &str) -> Self {
        let (host, name) = (host.to_string(), name.to_string());
        match kind {
            ItemKind::Workload => Action::BuildWorkload { host, name },
            ItemKind::Service => Action::BuildService { host, name },
            ItemKind::Ingress => Action::BuildIngress { host, name },
        }
    }

    fn teardown(kind: ItemKind, host: &str, name: &str) -> Self {
        let (host, name) = (host.to_string(), name.to_string());
        match kind {
            ItemKind::Workload => Action::TeardownWorkload { host, name },
            ItemKind::Service => Action::TeardownService { host, name },
            ItemKind::Ingress => Action::TeardownIngress { host, name },
        }
    }

    /// Which host this Action lands on.
    pub fn host(&self) -> &str {
        match self {
            Action::BuildWorkload { host, .. }
            | Action::TeardownWorkload { host, .. }
            | Action::BuildService { host, .. }
            | Action::TeardownService { host, .. }
            | Action::BuildIngress { host, .. }
            | Action::TeardownIngress { host, .. } => host,
        }
    }
}

/// What kind of change a [`Revision`] records.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RevisionKind {
    Init,
    Commission,
    Decommission,
}

/// One append-only entry in the journal.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Revision {
    pub id: u64,
    pub created_at: DateTime<Utc>,
    pub kind: RevisionKind,
    pub blueprint: Option<String>,
    pub actions: Vec<Action>,
    pub state: State,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bp(name: &str, host: &str, services: &[&str]) -> Blueprint {
        Blueprint {
            name: name.into(),
            hosts: vec![Host {
                name: host.into(),
                services: services
                    .iter()
                    .map(|s| Service { name: s.to_string(), ..Default::default() })
                    .collect(),
                ..Default::default()
            }],
        }
    }

    fn active(bps: &[Blueprint]) -> BTreeMap<String, Blueprint> {
        bps.iter().map(|b| (b.name.clone(), b.clone())).collect()
    }

    #[test]
    fn resolve_tracks_owning_blueprints() {
        let st = State::resolve(&active(&[bp("a", "h1", &["nginx"]), bp("b", "h1", &["nginx", "pg"])]));
        assert_eq!(st.hosts["h1"].services["nginx"], BTreeSet::from(["a".into(), "b".into()]));
        assert_eq!(st.hosts["h1"].services["pg"], BTreeSet::from(["b".to_string()]));
    }

    #[test]
    fn commission_builds_only_new_items() {
        let s1 = State::resolve(&active(&[bp("a", "h1", &["nginx"])]));
        let s2 = State::resolve(&active(&[bp("a", "h1", &["nginx"]), bp("b", "h1", &["nginx", "pg"])]));
        // nginx already present (a wants it); only pg is newly built.
        assert_eq!(
            s2.actions_from(&s1),
            vec![Action::BuildService { host: "h1".into(), name: "pg".into() }]
        );
    }

    #[test]
    fn decommission_keeps_shared_tears_down_unique() {
        let both = active(&[bp("a", "h1", &["nginx"]), bp("b", "h1", &["nginx", "pg"])]);
        let s_both = State::resolve(&both);
        let mut only_a = both.clone();
        only_a.remove("b");
        let s_a = State::resolve(&only_a);
        // pg leaves; nginx stays (a still wants it).
        assert_eq!(
            s_a.actions_from(&s_both),
            vec![Action::TeardownService { host: "h1".into(), name: "pg".into() }]
        );
        assert!(s_a.hosts["h1"].services.contains_key("nginx"));
    }

    #[test]
    fn membership_only_change_produces_no_actions() {
        let s1 = State::resolve(&active(&[bp("a", "h1", &["nginx"])]));
        // b also wants nginx now, but nginx neither appears nor disappears.
        let s2 = State::resolve(&active(&[bp("a", "h1", &["nginx"]), bp("b", "h1", &["nginx"])]));
        assert!(s2.actions_from(&s1).is_empty());
    }

    #[test]
    fn diff_emits_the_right_variant_and_host_per_kind() {
        let bp = Blueprint {
            name: "a".into(),
            hosts: vec![Host {
                name: "h".into(),
                workloads: vec![Workload { name: "w".into(), ..Default::default() }],
                services: vec![Service { name: "s".into(), ..Default::default() }],
                ingress: vec![Ingress { name: "i".into(), ..Default::default() }],
            }],
        };
        let full = State::resolve(&active(&[bp]));

        let built = full.actions_from(&State::default());
        assert!(built.contains(&Action::BuildWorkload { host: "h".into(), name: "w".into() }));
        assert!(built.contains(&Action::BuildService { host: "h".into(), name: "s".into() }));
        assert!(built.contains(&Action::BuildIngress { host: "h".into(), name: "i".into() }));

        let torn = State::default().actions_from(&full);
        assert!(torn.contains(&Action::TeardownWorkload { host: "h".into(), name: "w".into() }));
        assert!(torn.contains(&Action::TeardownService { host: "h".into(), name: "s".into() }));
        assert!(torn.contains(&Action::TeardownIngress { host: "h".into(), name: "i".into() }));
    }
}
