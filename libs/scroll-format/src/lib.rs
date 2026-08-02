//! The wire contract between the compiler and the daemon: the glyph/scroll
//! data model, the binary content-addressed manifest, and the pure functions
//! that turn scrolls into that manifest and back.
//!
//! Pure data and pure functions — no filesystem, no network, no I/O of any
//! kind. `emet` (the compiler, the *writer*) and `golemd` (the daemon, the
//! *reader*) both depend on this crate; it depends on neither. Dependencies
//! point toward this small, stable centre, so the two ends share one
//! definition and cannot drift apart (ADR 0013 §1). Neither end's weight —
//! the compiler's parser, the daemon's server — leaks in here.
//!
//! The core invariant is content addressing: a scroll's identity is
//! `blake3(postcard::to_stdvec(scroll))` — the hash of the scroll ALONE, over
//! its deterministic postcard bytes, never over the manifest or its neighbours
//! (ADR 0012 §1). Postcard is non-self-describing, so a type's field and
//! variant order *is* the encoding; changing it changes every hash. The
//! serialized layout is versioned by [`FORMAT_VERSION`] and evolved in lockstep
//! by both ends (ADR 0012 §3).

pub mod content_id;
pub mod manifest;
pub mod scroll;

pub use content_id::{ContentId, ContentIdParseError};
pub use manifest::{
    check_format_version, content_id, content_id_of_glyph, from_bytes, to_bytes, to_json,
    AddressedScroll, FromBytesError, Manifest, FORMAT_VERSION,
};
pub use scroll::{
    Chunk, Contents, Entry, Glyph, LeafUnit, OnExhaust, Perms, Policy, Scroll, Secret, Text,
};
