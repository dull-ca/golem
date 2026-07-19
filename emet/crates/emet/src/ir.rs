//! The IR — the sole output of the language. There is no JSON/YAML step; the
//! language *is* the generator and this is what it generates.
//!
//! Every field is a plain, concrete `String`. The IR carries no templates,
//! placeholders, or DSL: all computation happens in the typed language and is
//! fully evaluated before a value reaches a glyph field (ADR 0004). A consumer
//! reconciles this inert data against a real machine.
//!
//! Two levels: a `Glyph` is one bottom-level OS resource; a `Scroll` groups
//! the glyphs for one host (ADR 0009).

/// A single OS-resource primitive. Adding a capability means a new variant
/// here plus a reconciler in the consumer — the *language* is untouched
/// (ADR 0002).
#[derive(Debug, Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scroll {
    pub name: String,
    pub glyphs: Vec<Glyph>,
}

impl Scroll {
    pub fn describe(&self) -> String {
        format!("scroll `{}` ({} glyphs)", self.name, self.glyphs.len())
    }
}

impl Glyph {
    /// A namespaced identity string for conflict detection: two glyphs with
    /// the same key in one scroll must be identical or `analyze` rejects them
    /// (see `lib::analyze`).
    pub fn key(&self) -> String {
        match self {
            Glyph::AptPackage { name } => format!("apt:{name}"),
            Glyph::SystemdService { unit } => format!("systemd:{unit}"),
            Glyph::File { path, .. } => format!("file:{path}"),
            Glyph::LineInFile { path, line } => format!("fileline:{path}:{line}"),
        }
    }
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
