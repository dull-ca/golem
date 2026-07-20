//! Shared test helpers for the `List Scroll` output model. Most suites assert
//! over the glyphs of a single scroll; `single_scroll_glyphs` compiles, asserts
//! exactly one scroll, and hands back that scroll's glyphs so glyph-level
//! assertions stay unchanged.
#![allow(dead_code)]

use emet::{compile, ir::Glyph, Error, Phase};

/// Wrap glyph source (the old `[ <glyphs> ]` body) into a one-scroll `main`.
pub fn one_scroll(glyphs_src: &str) -> String {
    format!("main : List Scroll\nmain = [ scroll {{ name = \"test\", glyphs = {glyphs_src} }} ]\n")
}

/// Compile source that already produces a `List Scroll`, assert exactly one
/// scroll, and return that scroll's glyphs.
pub fn single_scroll_glyphs(src: &str) -> Vec<Glyph> {
    match compile(src) {
        Ok(c) => {
            assert_eq!(c.scrolls.len(), 1, "expected exactly one scroll");
            c.scrolls.into_iter().next().unwrap().glyphs
        }
        Err(e) => panic!("expected success, got {:?}: {}", e.phase, e.msg),
    }
}

/// Compile a bare glyph list `[ <glyphs> ]`, wrapping it in a single scroll, and
/// return that scroll's glyphs.
pub fn glyphs(glyphs_src: &str) -> Vec<Glyph> {
    single_scroll_glyphs(&one_scroll(glyphs_src))
}

pub fn err(src: &str) -> Error {
    match compile(src) {
        Ok(_) => panic!("expected an error, but compilation succeeded"),
        Err(e) => e,
    }
}

pub fn err_phase(src: &str) -> Phase {
    err(src).phase
}
