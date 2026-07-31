//! The glyph/scroll data model — the fully-evaluated, concrete desired state a
//! program compiles to. Every field is a plain `String`: the IR carries no
//! templates, placeholders, or DSL, because all computation happens in the
//! typed language and is fully evaluated before a value reaches a glyph field
//! (ADR 0004). A consumer reconciles this inert data against a real machine.
//!
//! A [`Glyph`] is one bottom-level OS resource; a [`Scroll`] is a recursive tree
//! of them for one host — the root is the host, interior branches are
//! subsystems, leaves are units of glyphs (ADR 0009, ADR 0031). The compiler
//! (`emet`) re-exports both through `emet::ir`.

use serde::{Deserialize, Serialize};

/// A single OS-resource primitive. Adding a capability means a new variant
/// here plus a reconciler in the consumer — the *language* is untouched
/// (ADR 0002).
///
// NOTE: variant order and each variant's field order ARE the postcard
// encoding (postcard is non-self-describing). Reordering or adding a
// variant/field is a `format_version`-bumping change, not a free refactor —
// see ADR 0012/0013.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Glyph {
    /// An apt package to install. Key `apt:<name>`.
    AptPackage { name: String },
    /// A systemd unit to enable and start. Key `systemd:<unit>`.
    SystemdService { unit: String },
    /// A filesystem entry at `path` — a file, a directory, or a symlink,
    /// selected by the [`Entry`] sum. One reconciler kind, one `key()` namespace
    /// (`file:<path>`) regardless of entry kind: one entry per path is one
    /// resource. `path` is the only field common to every entry, so it lives on
    /// the glyph; everything else lives inside the arm that gives it meaning.
    /// This generalizes ADR 0002's bare `file` glyph without adding a fifth
    /// reconciler kind — `Directory`/`Symlink` are variants of the *entry*, not
    /// new glyphs (ADR 0019).
    Filesystem { path: String, entry: Entry },
    /// A single line ensured present in a file. Key `fileline:<path>:<line>`.
    LineInFile { path: String, line: String },
}

/// What lives at a [`Glyph::Filesystem`]'s `path`: a file, a directory, or a
/// symlink (ADR 0019). Each arm carries *only* the fields that arm gives
/// meaning — `contents` and `perms` on `File`, `perms` alone on `Directory`,
/// `target` alone on `Symlink` — so the meaningless combinations cannot be
/// written down: a symlink has no `perms` field to hold a mode Linux would not
/// honour, and a directory has no `contents`. That is the "make illegal states
/// unrepresentable" discipline (ADR 0019 §1); it is why this is a sum of
/// minimal records rather than one record with fields that only sometimes apply.
///
// NOTE: variant order and each variant's field order ARE the postcard encoding
// (postcard is non-self-describing). Reordering a variant or a field — or
// adding an entry kind (device node, fifo, …) — is a `format_version`-bumping
// change, not a free refactor. Adding this sum in place of the flat `File`
// glyph is what took the manifest from format_version 1 to 2 (ADR 0012/0013/0019).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Entry {
    File { contents: String, perms: Perms },
    Directory { perms: Perms },
    Symlink { target: String },
}

/// The permission bits and optional ownership of a filesystem entry that has
/// them — carried only on the [`Entry::File`] and [`Entry::Directory`] arms,
/// never on [`Entry::Symlink`] (a symlink's own mode is not honoured on Linux).
/// `mode` is the 12 permission bits (setuid/setgid/sticky + rwxrwxrwx) as a
/// `u16`, parsed from an octal literal once in `emet` — a malformed mode is a
/// compile error, not a reconcile-time failure (ADR 0019 §1). `owner`/`group`
/// are names (portable across hosts in a way a raw uid is not) resolved to
/// uid/gid at reconcile time; `None` means "leave ownership as-is".
///
// NOTE: field order IS the postcard encoding — see the `Entry` note above.
// `mode` is the first non-`String`, non-flat glyph field in the wire model
// (every other field is a plain `String`, ADR 0004).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Perms {
    pub mode: u16,
    pub owner: Option<String>,
    pub group: Option<String>,
}

/// One host's worth of desired state, as a recursive, strict tree (ADR 0031).
/// The root scroll is the host — selected by `name` against `--host` — and every
/// scroll at any depth is a failure-isolation boundary carrying an optional
/// retry/rollback [`Policy`]. A [leaf](Scroll::is_leaf) (one holding glyphs) is
/// the unit of best-effort enact, retry, and rollback; a branch only groups its
/// sub-scrolls and cascades policy to the leaves beneath it (ADR 0031 §2/§3).
///
/// A `Scroll` is what gets content-addressed: its deterministic postcard bytes
/// are hashed to a [`ContentId`](crate::ContentId) (see
/// [`content_id()`](crate::content_id())). That hash now covers `policy` and
/// `contents`, so a policy edit or a regrouping is a different scroll — but no
/// glyph's own content id depends on its enclosing scroll, so regrouping
/// re-enacts nothing (ADR 0031 §4/§5).
///
/// The same glyph key may appear in two different scrolls without conflict (two
/// hosts installing `nginx`); a conflict is only within one leaf. `name` is a
/// label — no cross-scroll uniqueness is enforced (ADR 0009).
///
// NOTE: field order IS the postcard encoding. Reordering or adding a field is a
// `format_version`-bumping change, not a free refactor — see ADR 0012/0013.
// Making `Scroll` recursive is what took the manifest from v2 to v3 (ADR 0031 §5);
// adding `notifies` is what took it from v3 to v4 (ADR 0036).
// NOTE: no `Eq` — `Policy` carries `f64` knobs (ADR 0031 §3), which are only
// `PartialEq`. Don't add `Eq` back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scroll {
    pub name: String,
    pub policy: Option<Policy>,
    /// Systemd units to reload once any glyph in or under this scroll lands
    /// changed. Unlike `policy`'s nearest-wins cascade, a branch's list *unions*
    /// down over every descendant leaf — reload obligations accumulate rather
    /// than override. Empty is the natural zero, so `Vec` rather than `Option`.
    /// Lives inside the hashed scroll but outside every glyph, which is the
    /// whole reason it sits here: rewiring a notification must not perturb a
    /// glyph's content id and force a spurious re-write (ADR 0036).
    pub notifies: Vec<String>,
    pub contents: Contents,
}

/// A scroll level is *either* a leaf's glyphs *or* a branch's named sub-scrolls,
/// never a mix. Being a sum makes a mixed level unrepresentable — the ADR 0019
/// "illegal states unrepresentable" discipline applied to grouping: there are no
/// loose glyphs alongside sub-scrolls, so "does this glyph belong to the group,
/// before it, or after it?" cannot arise (ADR 0031 §1). A loose glyph beside
/// sub-scrolls must be wrapped in its own one-glyph leaf.
///
// NOTE: variant order IS the postcard encoding — `Glyphs` then `Groups`.
// Reordering is a `format_version`-bumping change — see ADR 0031 §5.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Contents {
    Glyphs(Vec<Glyph>),
    Groups(Vec<Scroll>),
}

/// The retry knobs and `on_exhaust` decision governing a leaf unit's enact
/// (ADR 0029 §3, ADR 0031 §3). Every field is optional and inherits when absent:
/// the effective policy for a leaf is resolved nearest-wins over its
/// [`policy_chain`](LeafUnit::policy_chain) plus the `golemd.toml` fallback — a
/// consumer concern, not resolved here. `on_exhaust` defaults to `Rollback`
/// (ADR 0029 §4). Lives inside the hashed scroll but never inside a glyph, so it
/// never perturbs a glyph's content id (ADR 0031 §5).
///
// NOTE: field order IS the postcard encoding — see ADR 0031 §5 for the fixed
// order. `backoff_multiplier` and `jitter_fraction` are `f64`; they are why no
// type in this tree can derive `Eq`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Policy {
    pub base_delay_ms: Option<u64>,
    pub backoff_multiplier: Option<f64>,
    pub max_delay_ms: Option<u64>,
    pub jitter_fraction: Option<f64>,
    pub max_attempts: Option<u32>,
    pub max_elapsed_ms: Option<u64>,
    pub on_exhaust: Option<OnExhaust>,
}

/// What a leaf unit does when it exhausts its retry budget with glyphs still
/// failing: `Rollback` returns the unit to its last committed state (the ADR
/// 0029 §4 default), `Keep` leaves partial progress in place for units that
/// prefer forward progress over atomicity. Scoped to the exhausting unit's
/// subtree alone — never a sibling (ADR 0031 §2).
///
// NOTE: variant order IS the postcard encoding — `Rollback` then `Keep`.
// See ADR 0031 §5.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OnExhaust {
    Rollback,
    Keep,
}

/// A leaf scroll flattened out for enact: its root-to-leaf name-`path`, its
/// `glyphs`, the `policy_chain` of every ancestor policy in the same
/// root-to-leaf order, and the `notifies` union over that same chain. This is
/// not a wire type — it is the shape [`Scroll::leaf_units`] hands a consumer,
/// which resolves the effective policy nearest-wins over `policy_chain` and
/// reports outcomes under `path` (ADR 0031 §2/§3/§4).
///
/// `policy_chain` arrives unresolved and `notifies` arrives resolved for the
/// same reason: the effective policy still needs the consumer's own
/// `golemd.toml` fallback folded in, and a union has no fallback to wait for.
pub struct LeafUnit<'a> {
    pub path: Vec<String>,
    pub glyphs: &'a [Glyph],
    pub policy_chain: Vec<&'a Policy>,
    pub notifies: Vec<String>,
}

/// Union `added` into `accumulated`, append-if-absent so the result keeps
/// root-to-leaf first-mention order, returning how many entries this scope
/// actually contributed. The count is what lets a depth-first walk truncate
/// exactly its own contribution on the way back up, so a sibling never inherits
/// a sibling's units.
fn extend_notifies(accumulated: &mut Vec<String>, added: &[String]) -> usize {
    let before = accumulated.len();
    for unit in added {
        if !accumulated.contains(unit) {
            accumulated.push(unit.clone());
        }
    }
    accumulated.len() - before
}

impl Scroll {
    /// Whether this scroll directly holds glyphs (a unit of enact) rather than
    /// grouping sub-scrolls.
    pub fn is_leaf(&self) -> bool {
        matches!(self.contents, Contents::Glyphs(_))
    }

    /// This scroll's own glyphs, or `&[]` for a branch — a branch has no glyphs
    /// of its own by design (ADR 0031 §1). To reach the glyphs under a branch,
    /// use [`all_glyphs`](Scroll::all_glyphs) or [`leaf_units`](Scroll::leaf_units).
    pub fn glyphs(&self) -> &[Glyph] {
        match &self.contents {
            Contents::Glyphs(g) => g,
            Contents::Groups(_) => &[],
        }
    }

    /// Every glyph in the subtree, flattened depth-first in source order —
    /// the position-independent desired set the diff keys on, identical however
    /// the glyphs are grouped (ADR 0031 §4).
    pub fn all_glyphs(&self) -> Vec<&Glyph> {
        let mut out = Vec::new();
        self.collect_glyphs(&mut out);
        out
    }

    fn collect_glyphs<'a>(&'a self, out: &mut Vec<&'a Glyph>) {
        match &self.contents {
            Contents::Glyphs(g) => out.extend(g.iter()),
            Contents::Groups(children) => {
                for child in children {
                    child.collect_glyphs(out);
                }
            }
        }
    }

    /// Every leaf unit in the subtree, in source order, each with its
    /// root-to-leaf name-path and ancestor policy chain. The unit of
    /// best-effort enact, retry, and rollback (ADR 0031 §2); the caller resolves
    /// the effective policy nearest-wins over each unit's chain.
    pub fn leaf_units(&self) -> Vec<LeafUnit<'_>> {
        let mut out = Vec::new();
        self.collect_leaves(&mut Vec::new(), &mut Vec::new(), &mut Vec::new(), &mut out);
        out
    }

    fn collect_leaves<'a>(
        &'a self,
        path: &mut Vec<String>,
        policy_chain: &mut Vec<&'a Policy>,
        notifies: &mut Vec<String>,
        out: &mut Vec<LeafUnit<'a>>,
    ) {
        path.push(self.name.clone());
        if let Some(p) = &self.policy {
            policy_chain.push(p);
        }
        let added = extend_notifies(notifies, &self.notifies);
        match &self.contents {
            Contents::Glyphs(g) => out.push(LeafUnit {
                path: path.clone(),
                glyphs: g,
                policy_chain: policy_chain.clone(),
                notifies: notifies.clone(),
            }),
            Contents::Groups(children) => {
                for child in children {
                    child.collect_leaves(path, policy_chain, notifies, out);
                }
            }
        }
        notifies.truncate(notifies.len() - added);
        if self.policy.is_some() {
            policy_chain.pop();
        }
        path.pop();
    }

    /// The same root-to-leaf `notifies` union [`leaf_units`](Scroll::leaf_units)
    /// resolves, but recorded at *every* node — branches included — so a consumer
    /// can answer for a path that is not a leaf of the tree. That is exactly the
    /// vanished-unit case: a `<removes>` group reports under a surviving
    /// ancestor's path, which may well be a branch (ADR 0036).
    pub fn notifies_by_path(&self) -> Vec<(Vec<String>, Vec<String>)> {
        let mut out = Vec::new();
        self.collect_notifies(&mut Vec::new(), &mut Vec::new(), &mut out);
        out
    }

    fn collect_notifies(
        &self,
        path: &mut Vec<String>,
        notifies: &mut Vec<String>,
        out: &mut Vec<(Vec<String>, Vec<String>)>,
    ) {
        path.push(self.name.clone());
        let added = extend_notifies(notifies, &self.notifies);
        out.push((path.clone(), notifies.clone()));
        if let Contents::Groups(children) = &self.contents {
            for child in children {
                child.collect_notifies(path, notifies, out);
            }
        }
        notifies.truncate(notifies.len() - added);
        path.pop();
    }

    /// A one-line human summary of the scroll — its glyph count if a leaf, its
    /// group count if a branch (the `--text` plan view, not the wire contract).
    pub fn describe(&self) -> String {
        match &self.contents {
            Contents::Glyphs(g) => format!("scroll `{}` ({} glyphs)", self.name, g.len()),
            Contents::Groups(children) => {
                format!("scroll `{}` ({} groups)", self.name, children.len())
            }
        }
    }
}

impl Glyph {
    /// A namespaced identity string for conflict detection: two glyphs with
    /// the same key in one scroll must be identical or `analyze` rejects them
    /// (see `emet`'s `lib::analyze`). Not part of the wire contract.
    pub fn key(&self) -> String {
        match self {
            Glyph::AptPackage { name } => format!("apt:{name}"),
            Glyph::SystemdService { unit } => format!("systemd:{unit}"),
            Glyph::Filesystem { path, .. } => format!("file:{path}"),
            Glyph::LineInFile { path, line } => format!("fileline:{path}:{line}"),
        }
    }

    /// A one-line human summary of the glyph (the `--text` plan view, not the
    /// wire contract).
    pub fn describe(&self) -> String {
        match self {
            Glyph::AptPackage { name } => {
                format!("ensure apt package `{name}` installed")
            }
            Glyph::SystemdService { unit } => {
                format!("enable + start systemd unit `{unit}`")
            }
            Glyph::Filesystem { path, entry } => match entry {
                Entry::File { perms, .. } => {
                    format!("ensure file `{path}` (mode {:04o})", perms.mode)
                }
                Entry::Directory { perms } => {
                    format!("ensure directory `{path}` (mode {:04o})", perms.mode)
                }
                Entry::Symlink { target } => {
                    format!("ensure symlink `{path}` -> `{target}`")
                }
            },
            Glyph::LineInFile { path, line } => {
                format!("ensure line `{line}` present in file `{path}`")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apt(name: &str) -> Glyph {
        Glyph::AptPackage {
            name: name.to_string(),
        }
    }

    fn leaf(name: &str, glyphs: Vec<Glyph>) -> Scroll {
        Scroll {
            name: name.to_string(),
            policy: None,
            notifies: vec![],
            contents: Contents::Glyphs(glyphs),
        }
    }

    fn branch(name: &str, groups: Vec<Scroll>) -> Scroll {
        Scroll {
            name: name.to_string(),
            policy: None,
            notifies: vec![],
            contents: Contents::Groups(groups),
        }
    }

    fn notifying(mut scroll: Scroll, units: &[&str]) -> Scroll {
        scroll.notifies = units.iter().map(|u| u.to_string()).collect();
        scroll
    }

    #[test]
    fn leaf_reports_its_glyphs_and_is_a_leaf() {
        let s = leaf("db", vec![apt("postgresql")]);
        assert!(s.is_leaf());
        assert_eq!(s.glyphs(), &[apt("postgresql")]);
    }

    #[test]
    fn branch_has_no_glyphs_and_is_not_a_leaf() {
        let s = branch("host", vec![leaf("db", vec![apt("postgresql")])]);
        assert!(!s.is_leaf());
        assert_eq!(s.glyphs(), &[] as &[Glyph]);
    }

    #[test]
    fn leaf_units_walk_source_order_with_name_paths() {
        let host = branch(
            "worker-01",
            vec![
                branch(
                    "fishnet",
                    vec![
                        leaf("client-1", vec![apt("stockfish")]),
                        leaf("client-2", vec![apt("stockfish")]),
                    ],
                ),
                leaf("base", vec![apt("htop")]),
            ],
        );
        let units = host.leaf_units();
        let paths: Vec<Vec<String>> = units.iter().map(|u| u.path.clone()).collect();
        assert_eq!(
            paths,
            vec![
                vec![
                    "worker-01".to_string(),
                    "fishnet".to_string(),
                    "client-1".to_string()
                ],
                vec![
                    "worker-01".to_string(),
                    "fishnet".to_string(),
                    "client-2".to_string()
                ],
                vec!["worker-01".to_string(), "base".to_string()],
            ]
        );
        assert_eq!(units[2].glyphs, &[apt("htop")]);
    }

    #[test]
    fn policy_chain_is_root_to_leaf() {
        let child = Scroll {
            name: "client-2".to_string(),
            policy: Some(Policy {
                on_exhaust: Some(OnExhaust::Keep),
                ..Policy::default()
            }),
            notifies: vec![],
            contents: Contents::Glyphs(vec![apt("stockfish")]),
        };
        let host = Scroll {
            name: "worker".to_string(),
            policy: Some(Policy {
                max_attempts: Some(9),
                ..Policy::default()
            }),
            notifies: vec![],
            contents: Contents::Groups(vec![child]),
        };
        let units = host.leaf_units();
        assert_eq!(units.len(), 1);
        assert_eq!(units[0].policy_chain.len(), 2);
        assert_eq!(units[0].policy_chain[0].max_attempts, Some(9));
        assert_eq!(units[0].policy_chain[1].on_exhaust, Some(OnExhaust::Keep));
    }

    #[test]
    fn branch_notifies_union_down_to_every_leaf() {
        let host = notifying(
            branch(
                "web",
                vec![
                    notifying(leaf("nginx", vec![apt("nginx")]), &["nginx.service"]),
                    leaf("base", vec![apt("htop")]),
                ],
            ),
            &["telegraf.service"],
        );
        let units = host.leaf_units();
        assert_eq!(units[0].notifies, vec!["telegraf.service", "nginx.service"]);
        assert_eq!(units[1].notifies, vec!["telegraf.service"]);
    }

    #[test]
    fn a_unit_repeated_along_the_chain_is_listed_once() {
        let host = notifying(
            branch(
                "web",
                vec![notifying(
                    leaf("nginx", vec![apt("nginx")]),
                    &["nginx.service", "nginx.service"],
                )],
            ),
            &["nginx.service"],
        );
        assert_eq!(host.leaf_units()[0].notifies, vec!["nginx.service"]);
    }

    #[test]
    fn a_sibling_never_inherits_its_siblings_notifies() {
        let host = branch(
            "web",
            vec![
                notifying(leaf("a", vec![apt("one")]), &["a.service"]),
                leaf("b", vec![apt("two")]),
            ],
        );
        let units = host.leaf_units();
        assert_eq!(units[0].notifies, vec!["a.service"]);
        assert!(units[1].notifies.is_empty());
    }

    #[test]
    fn notifies_by_path_resolves_branches_as_well_as_leaves() {
        let host = notifying(
            branch(
                "web",
                vec![notifying(
                    leaf("nginx", vec![apt("nginx")]),
                    &["nginx.service"],
                )],
            ),
            &["telegraf.service"],
        );
        let by_path = host.notifies_by_path();
        assert_eq!(
            by_path,
            vec![
                (
                    vec!["web".to_string()],
                    vec!["telegraf.service".to_string()]
                ),
                (
                    vec!["web".to_string(), "nginx".to_string()],
                    vec!["telegraf.service".to_string(), "nginx.service".to_string()]
                ),
            ]
        );
    }

    #[test]
    fn all_glyphs_flattens_in_source_order() {
        let host = branch(
            "h",
            vec![
                leaf("a", vec![apt("one"), apt("two")]),
                leaf("b", vec![apt("three")]),
            ],
        );
        let flat: Vec<&Glyph> = host.all_glyphs();
        assert_eq!(flat, vec![&apt("one"), &apt("two"), &apt("three")]);
    }
}
