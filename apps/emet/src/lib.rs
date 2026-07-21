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

/// The result of an LSP-facing analysis pass: the diagnostics `compile` would
/// report, plus the position index the editor queries for hover, completion,
/// and go-to-definition (ADR 0018). `compile`/`emetc` never build this — the
/// index is populated only when inference runs with a recorder (`analyze_*`),
/// so the compile path carries no recording cost.
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
    parse_source_multi(src).map_err(|mut errors| errors.remove(0))
}

pub fn parse_source_multi(src: &str) -> Result<Module, Vec<Error>> {
    let tokens = lexer::lex(src).map_err(|e| {
        vec![Error {
            phase: Phase::Lex,
            msg: e.msg,
            span: e.span,
            note: None,
        }]
    })?;

    let header = header::split(tokens).map_err(|e| {
        vec![Error {
            phase: Phase::Parse,
            msg: e.msg,
            span: e.span,
            note: None,
        }]
    })?;

    let laid = layout::layout_all(header.body);
    parser::parse(&laid, header.name, header.exposing, header.imports).map_err(|errors| {
        errors
            .into_iter()
            .map(|pe| Error {
                phase: Phase::Parse,
                msg: pe.msg,
                span: pe.span,
                note: None,
            })
            .collect()
    })
}

/// Full pipeline for a single source string: source -> glyphs, or the first
/// error encountered. Imports are not resolved from disk here; use
/// `compile_file` for a multi-module program.
pub fn compile(src: &str) -> Result<Compiled, Error> {
    compile_all(src).map_err(|mut errors| errors.remove(0))
}

pub fn compile_all(src: &str) -> Result<Compiled, Vec<Error>> {
    let module = parse_source_multi(src)?;

    let (_, main_ty) = infer::check_module(&module).map_err(|e| {
        vec![Error {
            phase: Phase::Type,
            msg: e.msg,
            span: e.span,
            note: e.note,
        }]
    })?;

    let scrolls = eval::run_module(&module).map_err(|e| {
        vec![Error {
            phase: Phase::Analyze,
            msg: e.msg,
            span: 0..0,
            note: None,
        }]
    })?;

    analyze(&scrolls).map_err(|msg| {
        vec![Error {
            phase: Phase::Analyze,
            msg,
            span: 0..0,
            note: None,
        }]
    })?;

    Ok(Compiled { main_ty, scrolls })
}

/// Full pipeline for a multi-module program: load the entry file, discover and
/// load its imported modules from disk (file path = module name, relative to
/// the entry's directory), reject import cycles, then type-check and evaluate
/// each module against the interfaces of what it imports.
pub fn compile_file(entry: &Path) -> Result<Compiled, Error> {
    compile_file_all(entry).map_err(|mut errors| errors.remove(0))
}

pub fn compile_file_all(entry: &Path) -> Result<Compiled, Vec<Error>> {
    let (main_ty, scrolls) = resolve::compile_entry(entry).map_err(|mut errors| {
        if errors.is_empty() {
            errors.push(Error {
                phase: Phase::Analyze,
                msg: "no entry module produced".to_string(),
                span: 0..0,
                note: None,
            });
        }
        errors
    })?;
    Ok(Compiled { main_ty, scrolls })
}

/// Analyze one source string for the LSP: parse, then run inference with a
/// recorder to build the `QueryIndex` alongside any diagnostic. Single-file
/// (imports unresolved), the counterpart of `compile` on the tooling path;
/// `analyze_project` is the multi-module counterpart of `compile_file`.
pub fn analyze_source(src: &str) -> Analysis {
    let module = match parse_source_multi(src) {
        Ok(m) => m,
        Err(errors) => {
            return Analysis { diagnostics: errors, index: QueryIndex::default() };
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

/// Analyze a multi-module program for the LSP: resolve the import graph and
/// return a per-file `QueryIndex` plus diagnostics, so hover and cross-file
/// go-to-definition work across a project. The `analyze_source` of a whole
/// project.
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
