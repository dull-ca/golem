//! emet library crate: a small total, typed, functional configuration
//! language with Elm/Haskell-style surface syntax (top-level declarations,
//! optional type signatures, Hindley-Milner inference, the offside layout
//! rule), whose sole output is the glyph IR.

pub mod ast;
pub mod depgraph;
pub mod eval;
pub mod header;
pub mod infer;
pub mod ir;
pub mod layout;
pub mod lexer;
pub mod parser;
pub mod prelude;
pub mod query;
pub mod resolve;

use std::path::Path;

use ast::Module;
use ast::Type;
use ir::{Glyph, Scroll};
use query::QueryIndex;

pub struct Analysis {
    pub diagnostics: Vec<Error>,
    pub index: QueryIndex,
}

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

#[derive(Debug)]
pub struct Compiled {
    pub main_ty: Type,
    pub scrolls: Vec<Scroll>,
}

/// Lex, split off any module header + imports, lay out the body, and parse it
/// into a `Module`. The header is optional: a header-less file defaults to an
/// unnamed module exposing everything, so a single-file program compiles
/// exactly as before.
pub fn parse_source(src: &str) -> Result<Module, Error> {
    let tokens = lexer::lex(src).map_err(|e| Error {
        phase: Phase::Lex,
        msg: e.msg,
        span: e.span,
        note: None,
    })?;

    let header = header::split(tokens).map_err(|e| Error {
        phase: Phase::Parse,
        msg: e.msg,
        span: e.span,
        note: None,
    })?;

    let laid = layout::layout_all(header.body);
    parser::parse(&laid, header.name, header.exposing, header.imports).map_err(|mut errors| {
        let first = errors.remove(0);
        Error {
            phase: Phase::Parse,
            msg: first.msg,
            span: first.span,
            note: None,
        }
    })
}

/// Full pipeline for a single source string: source -> glyphs, or the first
/// error encountered. Imports are not resolved from disk here; use
/// `compile_file` for a multi-module program.
pub fn compile(src: &str) -> Result<Compiled, Error> {
    let module = parse_source(src)?;

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

/// Full pipeline for a multi-module program: load the entry file, discover and
/// load its imported modules from disk (file path = module name, relative to
/// the entry's directory), reject import cycles, then type-check and evaluate
/// each module against the interfaces of what it imports.
pub fn compile_file(entry: &Path) -> Result<Compiled, Error> {
    let (main_ty, scrolls) = resolve::compile_entry(entry)?;
    Ok(Compiled { main_ty, scrolls })
}

pub fn analyze_source(src: &str) -> Analysis {
    let module = match parse_source(src) {
        Ok(m) => m,
        Err(e) => {
            return Analysis { diagnostics: vec![e], index: QueryIndex::default() };
        }
    };
    let no_imports = std::collections::HashMap::new();
    let no_ctors = infer::ImportedConstructors::default();
    let (error, index) = infer::analyze_module(
        &module,
        prelude::ty_env(),
        &no_imports,
        &no_ctors,
        std::collections::HashMap::new(),
        0..src.len(),
    );
    let diagnostics = error
        .map(|e| {
            vec![Error {
                phase: Phase::Type,
                msg: e.msg,
                span: e.span,
                note: e.note,
            }]
        })
        .unwrap_or_default();
    Analysis { diagnostics, index }
}

pub fn analyze_project(entry: &Path) -> resolve::ProjectAnalysis {
    resolve::analyze_entry(entry)
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
