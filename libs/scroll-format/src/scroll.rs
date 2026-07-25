//! The glyph/scroll data model — the fully-evaluated, concrete desired state a
//! program compiles to. Every field is a plain `String`: the IR carries no
//! templates, placeholders, or DSL, because all computation happens in the
//! typed language and is fully evaluated before a value reaches a glyph field
//! (ADR 0004). A consumer reconciles this inert data against a real machine.
//!
//! Two levels: a [`Glyph`] is one bottom-level OS resource; a [`Scroll`] groups
//! the glyphs for one host (ADR 0009). The compiler (`emet`) re-exports both
//! through `emet::ir`.

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scroll {
    pub name: String,
    pub policy: Option<Policy>,
    pub contents: Contents,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Contents {
    Glyphs(Vec<Glyph>),
    Groups(Vec<Scroll>),
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OnExhaust {
    Rollback,
    Keep,
}

pub struct LeafUnit<'a> {
    pub path: Vec<String>,
    pub glyphs: &'a [Glyph],
    pub policy_chain: Vec<&'a Policy>,
}

impl Scroll {
    pub fn is_leaf(&self) -> bool {
        matches!(self.contents, Contents::Glyphs(_))
    }

    pub fn glyphs(&self) -> &[Glyph] {
        match &self.contents {
            Contents::Glyphs(g) => g,
            Contents::Groups(_) => &[],
        }
    }

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

    pub fn leaf_units(&self) -> Vec<LeafUnit<'_>> {
        let mut out = Vec::new();
        self.collect_leaves(&mut Vec::new(), &mut Vec::new(), &mut out);
        out
    }

    fn collect_leaves<'a>(
        &'a self,
        path: &mut Vec<String>,
        policy_chain: &mut Vec<&'a Policy>,
        out: &mut Vec<LeafUnit<'a>>,
    ) {
        path.push(self.name.clone());
        if let Some(p) = &self.policy {
            policy_chain.push(p);
        }
        match &self.contents {
            Contents::Glyphs(g) => out.push(LeafUnit {
                path: path.clone(),
                glyphs: g,
                policy_chain: policy_chain.clone(),
            }),
            Contents::Groups(children) => {
                for child in children {
                    child.collect_leaves(path, policy_chain, out);
                }
            }
        }
        if self.policy.is_some() {
            policy_chain.pop();
        }
        path.pop();
    }

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
        Glyph::AptPackage { name: name.to_string() }
    }

    fn leaf(name: &str, glyphs: Vec<Glyph>) -> Scroll {
        Scroll { name: name.to_string(), policy: None, contents: Contents::Glyphs(glyphs) }
    }

    fn branch(name: &str, groups: Vec<Scroll>) -> Scroll {
        Scroll { name: name.to_string(), policy: None, contents: Contents::Groups(groups) }
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
                    vec![leaf("client-1", vec![apt("stockfish")]), leaf("client-2", vec![apt("stockfish")])],
                ),
                leaf("base", vec![apt("htop")]),
            ],
        );
        let units = host.leaf_units();
        let paths: Vec<Vec<String>> = units.iter().map(|u| u.path.clone()).collect();
        assert_eq!(
            paths,
            vec![
                vec!["worker-01".to_string(), "fishnet".to_string(), "client-1".to_string()],
                vec!["worker-01".to_string(), "fishnet".to_string(), "client-2".to_string()],
                vec!["worker-01".to_string(), "base".to_string()],
            ]
        );
        assert_eq!(units[2].glyphs, &[apt("htop")]);
    }

    #[test]
    fn policy_chain_is_root_to_leaf() {
        let child = Scroll {
            name: "client-2".to_string(),
            policy: Some(Policy { on_exhaust: Some(OnExhaust::Keep), ..Policy::default() }),
            contents: Contents::Glyphs(vec![apt("stockfish")]),
        };
        let host = Scroll {
            name: "worker".to_string(),
            policy: Some(Policy { max_attempts: Some(9), ..Policy::default() }),
            contents: Contents::Groups(vec![child]),
        };
        let units = host.leaf_units();
        assert_eq!(units.len(), 1);
        assert_eq!(units[0].policy_chain.len(), 2);
        assert_eq!(units[0].policy_chain[0].max_attempts, Some(9));
        assert_eq!(units[0].policy_chain[1].on_exhaust, Some(OnExhaust::Keep));
    }

    #[test]
    fn all_glyphs_flattens_in_source_order() {
        let host = branch(
            "h",
            vec![leaf("a", vec![apt("one"), apt("two")]), leaf("b", vec![apt("three")])],
        );
        let flat: Vec<&Glyph> = host.all_glyphs();
        assert_eq!(flat, vec![&apt("one"), &apt("two"), &apt("three")]);
    }
}
