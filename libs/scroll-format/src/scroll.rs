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

/// One host's worth of desired state: a named container of glyphs. The same
/// glyph key may appear in two different scrolls without conflict (two hosts
/// installing `nginx`); a conflict is only within one scroll. `name` is a
/// label for now — no cross-scroll uniqueness is enforced (ADR 0009).
///
/// A `Scroll` is what gets content-addressed: its deterministic postcard bytes
/// are hashed to a [`ContentId`](crate::ContentId) (see [`content_id()`](crate::content_id())).
///
// NOTE: field order IS the postcard encoding. Reordering or adding a field is
// a `format_version`-bumping change, not a free refactor — see ADR 0012/0013.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scroll {
    pub name: String,
    pub glyphs: Vec<Glyph>,
}

impl Scroll {
    /// A one-line human summary of the scroll (the `--text` plan view, not the
    /// wire contract).
    pub fn describe(&self) -> String {
        format!("scroll `{}` ({} glyphs)", self.name, self.glyphs.len())
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
