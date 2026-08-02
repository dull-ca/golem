//! The top-level artifact `emet` emits and `golemd` consumes: a
//! [`Manifest`] of content-addressed scrolls, plus the postcard/JSON
//! serialization helpers and the `format_version` guard.

use serde::{Deserialize, Serialize};

use crate::content_id::ContentId;
use crate::scroll::{Glyph, Scroll};

/// The wire-contract version of the serialized layout. Bumped whenever the
/// manifest shape, a `Scroll`/`Glyph` field or variant, the postcard format,
/// or the BLAKE3 hash changes — anything that alters the bytes either end
/// reads (ADR 0012 §1). Distinct from `emet_version`, which is provenance and
/// is never hashed.
///
// NOTE: `4` because ADR 0036 added `notifies` to `Scroll`, between `policy` and
// `contents`. Postcard is non-self-describing, so a field addition IS a layout
// change: v3 bytes would misread the old `contents` tag as the new `notifies`
// length. No glyph changed shape, so every glyph content id survives the bump
// untouched and the first v4 apply is a Noop pass, not a Replace storm.
// (v3 was the recursive `Scroll` tree of ADR 0031 plus ADR 0030's enriched
// `aptPackage`; v2 was the filesystem glyph of ADR 0019.)
pub const FORMAT_VERSION: u32 = 5;

/// A scroll paired with its content address. The `content_id` is over the
/// `scroll` ALONE — never over this wrapper — so a scroll's identity does not
/// depend on its neighbours or on `emet_version` (ADR 0012 §1).
///
// NOTE: field order IS the postcard encoding. Reordering or adding a field is
// a `format_version`-bumping change, not a free refactor — see ADR 0012/0013.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AddressedScroll {
    pub content_id: ContentId,
    pub scroll: Scroll,
}

/// One compiled fleet: every scroll with its content ID, tagged with the wire
/// version and the compiler that produced it.
///
/// `format_version` versions the wire layout (checked on read, see
/// [`check_format_version`]); `emet_version` records which compiler build
/// wrote the manifest, is provenance only, and sits deliberately outside every
/// hash — rebuilding an unchanged fleet with a newer compiler yields the same
/// per-scroll content IDs (ADR 0012 §1).
///
// NOTE: field order IS the postcard encoding. Reordering or adding a field is
// a `format_version`-bumping change, not a free refactor — see ADR 0012/0013.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub format_version: u32,
    pub emet_version: String,
    pub scrolls: Vec<AddressedScroll>,
}

impl Manifest {
    /// Assemble a manifest from the compiled scrolls, content-addressing each
    /// and stamping the current [`FORMAT_VERSION`] and the given compiler
    /// version.
    pub fn from_scrolls(scrolls: Vec<Scroll>, emet_version: impl Into<String>) -> Manifest {
        let addressed = scrolls
            .into_iter()
            .map(|scroll| AddressedScroll {
                content_id: content_id(&scroll),
                scroll,
            })
            .collect();
        Manifest {
            format_version: FORMAT_VERSION,
            emet_version: emet_version.into(),
            scrolls: addressed,
        }
    }
}

/// The content address of a scroll: BLAKE3 over its deterministic postcard
/// bytes. Postcard makes the bytes canonical by construction, so an identical
/// `Scroll` always yields an identical [`ContentId`] — the property the whole
/// content-addressing scheme rests on (ADR 0012 §3).
pub fn content_id(scroll: &Scroll) -> ContentId {
    let bytes = postcard::to_stdvec(scroll).expect("scroll serialization is infallible");
    ContentId(*blake3::hash(&bytes).as_bytes())
}

/// The content address of a single glyph: BLAKE3 over its deterministic
/// postcard bytes, the same scheme [`content_id`] uses for a whole scroll. This
/// is the per-glyph identity `golemd`'s reconciler diffs on — same glyph bytes ⇒
/// same id ⇒ no-op, a changed field ⇒ new id ⇒ upgrade (ADR 0015 §2). golemd
/// calls this through `reconcile::glyph_content_id`, which delegates here so the
/// hash has exactly one definition (ADR 0012/0013).
///
// NOTE: like `content_id`, this is part of the wire contract — a glyph's
// field/variant order IS the postcard encoding, so reordering one is a
// `format_version`-bumping change, not a free refactor.
pub fn content_id_of_glyph(glyph: &Glyph) -> ContentId {
    let bytes = postcard::to_stdvec(glyph).expect("glyph serialization is infallible");
    ContentId(*blake3::hash(&bytes).as_bytes())
}

#[derive(Debug, PartialEq, Eq)]
pub enum FromBytesError {
    UnsupportedFormatVersion { found: u32, supported: u32 },
    Decode(String),
}

impl std::fmt::Display for FromBytesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FromBytesError::UnsupportedFormatVersion { found, supported } => write!(
                f,
                "unsupported manifest format_version {found} (this build supports {supported})"
            ),
            FromBytesError::Decode(msg) => write!(f, "manifest decode failed: {msg}"),
        }
    }
}

impl std::error::Error for FromBytesError {}

/// Serialize the manifest to its canonical postcard bytes — the artifact
/// `golemd` consumes.
pub fn to_bytes(manifest: &Manifest) -> Vec<u8> {
    postcard::to_stdvec(manifest).expect("manifest serialization is infallible")
}

/// Decode a manifest from postcard bytes, rejecting an unknown
/// `format_version` with a typed [`FromBytesError`] rather than a misparse.
///
// NOTE: the version is read off the front and checked BEFORE the body is
// decoded, because postcard is non-self-describing: under a newer layout an
// older artifact's body is not merely different but unparseable, so decoding
// first surfaces a stale manifest as an inscrutable serde error instead of
// "this build speaks v4, that file is v3". `format_version` is the leading
// field of `Manifest`, so its varint is the leading varint of the stream.
pub fn from_bytes(bytes: &[u8]) -> Result<Manifest, FromBytesError> {
    if let Ok((found, _)) = postcard::take_from_bytes::<u32>(bytes) {
        if (1..=MAX_PLAUSIBLE_FORMAT_VERSION).contains(&found) {
            check_format_version(found)?;
        }
    }
    postcard::from_bytes(bytes).map_err(|e| FromBytesError::Decode(e.to_string()))
}

// 31 is the last byte below printable ASCII, so no text file's leading varint
// can land in the band: garbage reports as undecodable, and only a version this
// build could plausibly meet reports as a version mismatch.
const MAX_PLAUSIBLE_FORMAT_VERSION: u32 = 31;

/// Accept a `format_version` only if it matches [`FORMAT_VERSION`]; any other
/// value is [`FromBytesError::UnsupportedFormatVersion`].
pub fn check_format_version(found: u32) -> Result<(), FromBytesError> {
    if found == FORMAT_VERSION {
        Ok(())
    } else {
        Err(FromBytesError::UnsupportedFormatVersion {
            found,
            supported: FORMAT_VERSION,
        })
    }
}

/// The self-describing JSON view of the manifest, for humans and ad-hoc
/// tooling (`--json`). A debug view only: its bytes are NOT content-addressed
/// and it is never the artifact `golemd` consumes (ADR 0012 §2).
pub fn to_json(manifest: &Manifest) -> String {
    serde_json::to_string_pretty(manifest).expect("manifest json view is infallible")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_version_is_five() {
        assert_eq!(FORMAT_VERSION, 5);
    }
}
