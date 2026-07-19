//! emet library crate: a small total, typed, functional configuration
//! language with Elm/Haskell-style surface syntax (top-level declarations,
//! optional type signatures, Hindley-Milner inference, the offside layout
//! rule), whose sole output is the glyph IR.

pub mod ast;
pub mod eval;
pub mod infer;
pub mod ir;
pub mod layout;
pub mod lexer;
pub mod parser;
pub mod prelude;

use ast::Type;
use ir::{Glyph, Scroll};

/// A compilation error, tagged by phase, with a source span for diagnostics.
#[derive(Debug)]
pub struct Error {
    pub phase: Phase,
    pub msg: String,
    pub span: std::ops::Range<usize>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Phase {
    Lex,
    Parse,
    Type,
    Analyze,
}

pub struct Compiled {
    pub main_ty: Type,
    pub scrolls: Vec<Scroll>,
}

/// Full pipeline: source -> glyphs, or the first error encountered.
pub fn compile(src: &str) -> Result<Compiled, Error> {
    let tokens = lexer::lex(src).map_err(|e| Error {
        phase: Phase::Lex,
        msg: e.msg,
        span: e.span,
        note: None,
    })?;

    let laid = layout::layout_all(tokens);
    let module = parser::parse(&laid).map_err(|mut errors| {
        let first = errors.remove(0);
        Error {
            phase: Phase::Parse,
            msg: first.msg,
            span: first.span,
            note: None,
        }
    })?;

    let (_, main_ty) = infer::check_module(&module).map_err(|e| Error {
        phase: Phase::Type,
        msg: e.msg,
        span: e.span,
        note: e.note,
    })?;

    let scrolls = eval::run_module(&module).map_err(|e| Error {
        phase: Phase::Analyze,
        msg: e.msg,
        span: 0..0,
        note: None,
    })?;

    analyze(&scrolls).map_err(|msg| Error {
        phase: Phase::Analyze,
        msg,
        span: 0..0,
        note: None,
    })?;

    Ok(Compiled { main_ty, scrolls })
}

/// Pre-apply analysis over the IR graph. It detects conflicting declarations
/// for the same glyph key within each scroll; two different scrolls may share
/// glyph keys without conflict. It is the hook where cycle checks and
/// conflicting-write checks live as the IR grows.
pub fn analyze(scrolls: &[Scroll]) -> Result<(), String> {
    use std::collections::HashMap;
    for scroll in scrolls {
        let mut seen: HashMap<String, &Glyph> = HashMap::new();
        for r in &scroll.glyphs {
            let k = r.key();
            if let Some(prev) = seen.get(&k) {
                if *prev != r {
                    return Err(format!("conflicting declarations for {k}"));
                }
            } else {
                seen.insert(k, r);
            }
        }
    }
    Ok(())
}
