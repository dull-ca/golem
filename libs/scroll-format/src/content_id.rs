//! The content address of a scroll: a 32-byte BLAKE3 digest of the scroll's
//! deterministic postcard bytes, with a lowercase-hex string form.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// The 32-byte BLAKE3 digest identifying a scroll by its content
/// (`blake3(postcard::to_stdvec(scroll))`; see [`content_id()`](crate::content_id())).
/// [`Display`](fmt::Display) renders it as lowercase hex and [`FromStr`] parses
/// that form back.
///
// NOTE: this is stored as the raw digest, not the hex string; the digest width
// and encoding are pinned by `format_version` — see ADR 0012/0013. The serde
// encoding is deliberately left to the derive so every format — the postcard
// manifest wire and golemd's serde_json WAL alike — stores the same raw digest;
// the hex form is a *display* concern (`Display`/`FromStr`), rendered by callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContentId(pub [u8; 32]);

impl ContentId {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
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
