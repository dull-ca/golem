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
pub mod manifest;
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
/// `file` names the module the span indexes into; `None` means the entry file
/// (the compile driver's default source), so a diagnostic always renders
/// against the source its span belongs to, not the entry file's.
#[derive(Debug)]
pub struct Error {
    pub phase: Phase,
    pub msg: String,
    pub span: std::ops::Range<usize>,
    pub note: Option<String>,
    pub file: Option<std::path::PathBuf>,
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

/// The multi-error variant of `parse_source` (ADR 0022). The parser recovers
/// past a malformed top-level declaration and reports every independent one, so
/// a failed parse comes back as a `Vec<Error>`. Lex and header failures are
/// still fatal and single — there is nothing to recover past before layout has
/// run — but they are wrapped in a one-element vec so the caller has one shape
/// to handle. Only parse-phase errors ever arrive as a list; later phases
/// (type, eval, analyze) stay first-error. `parse_source` is the first-error
/// wrapper kept for existing callers.
pub fn parse_source_multi(src: &str) -> Result<Module, Vec<Error>> {
    let tokens = lexer::lex(src).map_err(|e| {
        vec![Error {
            phase: Phase::Lex,
            msg: e.msg,
            span: e.span,
            note: None,
            file: None,
        }]
    })?;

    let header = header::split(tokens).map_err(|e| {
        vec![Error {
            phase: Phase::Parse,
            msg: e.msg,
            span: e.span,
            note: None,
            file: None,
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
                file: None,
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

/// The multi-error variant of `compile` (ADR 0022). The returned `Vec<Error>`
/// holds *either* several parse errors (the parser recovers at declaration
/// boundaries and collects all of them) *or* a single later-phase error — never
/// a mix. Type, eval, and analyze are sequential and unwind at the first
/// failure, so only the parse phase produces a list. `compile` remains the
/// first-error wrapper.
pub fn compile_all(src: &str) -> Result<Compiled, Vec<Error>> {
    let module = parse_source_multi(src)?;

    let (_, main_ty) = infer::check_module(&module).map_err(|e| {
        vec![Error {
            phase: Phase::Type,
            msg: e.msg,
            span: e.span,
            note: e.note,
            file: None,
        }]
    })?;

    let scrolls = eval::run_module(&module).map_err(|e| {
        vec![Error {
            phase: Phase::Analyze,
            msg: e.msg,
            span: e.span,
            note: None,
            file: None,
        }]
    })?;

    analyze(&scrolls).map_err(|msg| {
        vec![Error {
            phase: Phase::Analyze,
            msg,
            span: 0..0,
            note: None,
            file: None,
        }]
    })?;

    Ok(Compiled { main_ty, scrolls })
}

/// Full pipeline for a multi-module program: load the entry file, discover and
/// load its imported modules from disk (file path = module name, resolved over
/// the ADR 0024 search path — entry directory first, then the `emet.json`
/// library directories), reject import cycles, then type-check and evaluate each
/// module against the interfaces of what it imports.
pub fn compile_file(entry: &Path) -> Result<Compiled, Error> {
    compile_file_all(entry).map_err(|mut errors| errors.remove(0))
}

/// The multi-error variant of `compile_file` (ADR 0022). Each module in the
/// import graph is parsed through the recovering path, so a build reports every
/// parse error in the offending file; as in `compile_all`, the resulting
/// `Vec<Error>` is either several parse errors or one later-phase error.
/// `compile_file` remains the first-error wrapper.
pub fn compile_file_all(entry: &Path) -> Result<Compiled, Vec<Error>> {
    let (main_ty, scrolls) = resolve::compile_entry(entry).map_err(|mut errors| {
        if errors.is_empty() {
            errors.push(Error {
                phase: Phase::Analyze,
                msg: "no entry module produced".to_string(),
                span: 0..0,
                note: None,
                file: None,
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
            return Analysis {
                diagnostics: errors,
                index: QueryIndex::default(),
            };
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
                file: None,
            }]
        })
        .unwrap_or_default();
    Analysis { diagnostics, index }
}

/// Analyze a multi-module program for the LSP: resolve the import graph over the
/// same ADR 0024 search path as `compile_file` and return a per-file
/// `QueryIndex` plus diagnostics, so hover and cross-file go-to-definition work
/// across a project — including into library modules under `emet.json`'s
/// `source-directories`. The `analyze_source` of a whole project.
pub fn analyze_project(entry: &Path) -> resolve::ProjectAnalysis {
    resolve::analyze_entry(entry)
}

/// Pre-apply analysis over the IR graph. The conflict scope is the leaf unit,
/// not the whole scroll (ADR 0031 §1): each leaf is one conflict scope, so two
/// declarations of the same glyph key inside one leaf with differing bodies are
/// an error, while sibling leaves may carry the same key without conflict. It is
/// the hook where cycle checks and conflicting-write checks live as the IR grows.
pub fn analyze(scrolls: &[Scroll]) -> Result<(), String> {
    use std::collections::HashMap;
    for scroll in scrolls {
        for unit in scroll.leaf_units() {
            let mut seen: HashMap<String, &Glyph> = HashMap::new();
            for r in unit.glyphs {
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
    }
    Ok(())
}
