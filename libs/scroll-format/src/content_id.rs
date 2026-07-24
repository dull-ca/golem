//! The content address of a scroll: a 32-byte BLAKE3 digest of the scroll's
//! deterministic postcard bytes, with a lowercase-hex string form.

use std::fmt;
use std::str::FromStr;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The 32-byte BLAKE3 digest identifying a scroll by its content
/// (`blake3(postcard::to_stdvec(scroll))`; see [`content_id()`](crate::content_id())).
/// [`Display`](fmt::Display) renders it as lowercase hex and [`FromStr`] parses
/// that form back.
///
// NOTE: this is stored as the raw digest, not the hex string; the digest width
// and encoding are pinned by `format_version` — see ADR 0012/0013. The
// hand-written serde impls below keep that raw-digest wire encoding while
// letting human-readable formats emit hex.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContentId(pub [u8; 32]);

impl ContentId {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

// The derive is replaced by hand so one type can carry two encodings, chosen by
// the format's `is_human_readable()`:
//
//   - Human-readable formats (serde_json) get the lowercase-hex **string** —
//     the `Display`/`FromStr` form — so `fleet apply`, `--json` output, and
//     revisions read as a 64-char id rather than a 32-number array.
//   - Non-human-readable formats (postcard, the manifest wire format) get the
//     raw digest, encoded exactly as the derive would have: a newtype struct
//     over `[u8; 32]`.
//
// The postcard branch MUST stay byte-identical to the derived encoding — a
// content id is part of the non-self-describing manifest, so any change is a
// `format_version` bump, not a free refactor (ADR 0012/0013). That is why it
// goes through `serialize_newtype_struct`/`visit_newtype_struct` reading
// `<[u8; 32]>::deserialize` rather than re-encoding the bytes any other way.
// The determinism tests pin this: see `content_id_postcard_encoding_is_thirty_two_raw_bytes`
// and `manifest_postcard_bytes_are_unchanged_by_content_id_serde`.
impl Serialize for ContentId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            serializer.serialize_str(&self.to_string())
        } else {
            serializer.serialize_newtype_struct("ContentId", &self.0)
        }
    }
}

impl<'de> Deserialize<'de> for ContentId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if deserializer.is_human_readable() {
            let hex = String::deserialize(deserializer)?;
            hex.parse().map_err(de::Error::custom)
        } else {
            deserializer.deserialize_newtype_struct("ContentId", DigestVisitor)
        }
    }
}

struct DigestVisitor;

impl<'de> Visitor<'de> for DigestVisitor {
    type Value = ContentId;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a 32-byte content id")
    }

    fn visit_newtype_struct<D: Deserializer<'de>>(self, deserializer: D) -> Result<ContentId, D::Error> {
        Ok(ContentId(<[u8; 32]>::deserialize(deserializer)?))
    }
}

impl fmt::Display for ContentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&hex::encode(self.0))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ContentIdParseError {
    BadHex,
    WrongLength,
}

impl fmt::Display for ContentIdParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContentIdParseError::BadHex => f.write_str("content id is not valid hex"),
            ContentIdParseError::WrongLength => {
                f.write_str("content id must be 32 bytes (64 hex chars)")
            }
        }
    }
}

impl std::error::Error for ContentIdParseError {}

impl FromStr for ContentId {
    type Err = ContentIdParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = hex::decode(s).map_err(|_| ContentIdParseError::BadHex)?;
        let array: [u8; 32] = bytes
            .try_into()
            .map_err(|_| ContentIdParseError::WrongLength)?;
        Ok(ContentId(array))
    }
}
