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
    /// A file with fixed contents and mode. Key `file:<path>`.
    File { path: String, contents: String, mode: String },
    /// A single line ensured present in a file. Key `fileline:<path>:<line>`.
    LineInFile { path: String, line: String },
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
            Glyph::File { path, .. } => format!("file:{path}"),
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
            Glyph::File { path, mode, .. } => {
                format!("ensure file `{path}` (mode {mode})")
            }
            Glyph::LineInFile { path, line } => {
                format!("ensure line `{line}` present in file `{path}`")
            }
        }
    }
}
