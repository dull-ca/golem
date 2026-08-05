//! The IR the language compiles to. The `Glyph`/`Scroll` model itself lives in
//! the shared `scroll-format` crate — the wire contract between the compiler
//! and `golemd` (ADR 0013) — and `ir` re-exports it so the rest of the
//! compiler keeps treating these as "the IR." The substance of the model, and
//! why every field is inert concrete data, is documented there.

pub use scroll_format::{
    Chunk, Contents, Entry, Glyph, OnExhaust, Perms, Policy, Scroll, Secret, Text,
};
