//! The LSP feature layer (ADR 0018): translate an editor request at a cursor
//! into an `emet` query and shape the answer for LSP. It holds **no language
//! semantics** — every type, scope, definition, doc comment, and outline comes
//! from `emet`'s single inference engine, so the adapter and the compiler can
//! never disagree. Every feature enters through `analysis_of`, which picks the
//! project-aware `analyze_document` or single-file `analyze_source` by whether
//! the URI has a path; going through one door is what keeps hover, completion,
//! go-to-definition, and diagnostics from drifting apart. What lives here is
//! purely the LSP boundary: LSP UTF-16 positions ↔ byte offsets, `emet` facts ↔
//! LSP payloads, and the Markdown assembly for hover.

use std::path::PathBuf;

use lsp_types::{
    CompletionItem, CompletionItemKind, Diagnostic, DiagnosticSeverity, DocumentSymbol, Hover,
    HoverContents, Location, MarkupContent, MarkupKind, Position, Range, SymbolKind, Uri,
};

fn analysis_of(uri: &Uri, source: &str) -> emet::Analysis {
    match document_path(uri) {
        Some(path) => emet::analyze_document(&path, source),
        None => emet::analyze_source(source),
    }
}

fn document_path(uri: &Uri) -> Option<PathBuf> {
    uri.as_str().strip_prefix("file://").map(PathBuf::from)
}

/// What is under the cursor, as a Markdown hover: an expression's inferred type
/// plus its definition's doc comment and origin module, or — where no expression
/// was recorded — the declaration of the type name written there. `None` when
/// the position is neither.
pub fn hover_at(uri: &Uri, source: &str, position: Position) -> Option<Hover> {
    let analysis = analysis_of(uri, source);
    let offset = offset_at(source, position);
    match analysis.index.type_at(offset) {
        Some(ty) => {
            let described = analysis
                .index
                .definition_at(offset)
                .map(|site| describe_definition(uri, source, site))
                .unwrap_or_default();
            Some(markdown_hover(hover_markdown(&ty.to_string(), described)))
        }
        None => type_name_hover(uri, source, &analysis, offset),
    }
}

/// Hover's fallback for a type name — reached only when no recorded expression
/// covers the cursor, which is what leaves a constructor *use* on the value path
/// where its inferred type is the better answer. A type in an annotation, a
/// `type` declaration, or an `exposing` list has no expression span at all
/// (`ast::Type` carries none), so the name is recovered from the token stream.
fn type_name_hover(
    uri: &Uri,
    source: &str,
    analysis: &emet::Analysis,
    offset: usize,
) -> Option<Hover> {
    let name = emet::query::type_name_at(source, offset)?;
    match analysis.index.type_definitions.get(&name) {
        Some(definition) => Some(markdown_hover(hover_markdown(
            &definition.declaration,
            describe_definition(uri, source, &definition.site),
        ))),
        None => Some(markdown_hover(hover_markdown(
            &emet::query::builtin_type_declaration(&name)?,
            Described {
                doc: emet::query::builtin_type_doc(&name),
                origin: None,
            },
        ))),
    }
}

fn markdown_hover(value: String) -> Hover {
    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value,
        }),
        range: None,
    }
}

#[derive(Default)]
struct Described {
    doc: Option<String>,
    origin: Option<String>,
}

fn describe_definition(uri: &Uri, source: &str, site: &emet::query::DefSite) -> Described {
    match &site.module {
        None => Described {
            doc: emet::query::doc_comment_above_definition(source, &site.span),
            origin: None,
        },
        Some(module) => Described {
            doc: imported_module_source(uri, module)
                .and_then(|(_, text)| emet::query::doc_comment_above_definition(&text, &site.span)),
            origin: Some(module.clone()),
        },
    }
}

fn hover_markdown(declaration: &str, described: Described) -> String {
    let mut markdown = format!("```emet\n{declaration}\n```");
    if let Some(doc) = described.doc {
        markdown.push_str(&format!("\n\n{doc}"));
    }
    if let Some(origin) = described.origin {
        markdown.push_str(&format!("\n\n*from {origin}*"));
    }
    markdown
}

pub fn document_symbols(uri: &Uri, source: &str) -> Vec<DocumentSymbol> {
    let analysis = analysis_of(uri, source);
    emet::document_outline(source, &analysis.index)
        .into_iter()
        .map(|symbol| document_symbol(source, symbol))
        .collect()
}

fn document_symbol(source: &str, symbol: emet::query::Symbol) -> DocumentSymbol {
    let children: Vec<DocumentSymbol> = symbol
        .children
        .into_iter()
        .map(|child| document_symbol(source, child))
        .collect();
    #[allow(deprecated)]
    DocumentSymbol {
        name: symbol.name,
        detail: symbol.detail,
        kind: lsp_symbol_kind(&symbol.kind),
        tags: None,
        deprecated: None,
        range: span_to_range(source, &symbol.span),
        selection_range: span_to_range(source, &symbol.name_span),
        children: if children.is_empty() {
            None
        } else {
            Some(children)
        },
    }
}

fn lsp_symbol_kind(kind: &emet::query::SymbolKind) -> SymbolKind {
    match kind {
        emet::query::SymbolKind::Module => SymbolKind::MODULE,
        emet::query::SymbolKind::Value => SymbolKind::CONSTANT,
        emet::query::SymbolKind::Function => SymbolKind::FUNCTION,
        emet::query::SymbolKind::Type => SymbolKind::ENUM,
        emet::query::SymbolKind::Constructor => SymbolKind::ENUM_MEMBER,
    }
}

/// The names in scope at the cursor as completion items, each labeled with its
/// rendered type.
pub fn completion_at(uri: &Uri, source: &str, position: Position) -> Vec<CompletionItem> {
    let analysis = analysis_of(uri, source);
    let offset = offset_at(source, position);
    analysis
        .index
        .names_in_scope(offset)
        .into_iter()
        .map(|(name, scheme)| CompletionItem {
            label: name,
            detail: Some(scheme),
            kind: Some(CompletionItemKind::VALUE),
            ..Default::default()
        })
        .collect()
}

/// The definition site of the name at the cursor. A same-file definition
/// (`module: None`) resolves against `source`; a cross-file one names its owning
/// module, resolved to a sibling `<Module>.emet` file whose own text is read to
/// map the target span to a range.
pub fn definition_at(uri: &Uri, source: &str, position: Position) -> Option<Location> {
    let analysis = analysis_of(uri, source);
    let offset = offset_at(source, position);
    let site = analysis.index.definition_at(offset)?;
    match &site.module {
        None => Some(Location {
            uri: uri.clone(),
            range: span_to_range(source, &site.span),
        }),
        Some(module) => imported_module_location(uri, module, &site.span),
    }
}

/// Locate a cross-file definition: a definition in `module` lives in
/// `<module>.emet` somewhere on this document's ADR 0024 search path — its own
/// directory first, then the `emet.json` library directories, exactly where the
/// compiler's resolver found it. Its source is read so the byte span from the
/// exporter's interface can be mapped to a UTF-16 range in that file.
fn imported_module_location(
    uri: &Uri,
    module: &str,
    span: &std::ops::Range<usize>,
) -> Option<Location> {
    let (target_uri, target_source) = imported_module_source(uri, module)?;
    Some(Location {
        uri: target_uri,
        range: span_to_range(&target_source, span),
    })
}

fn imported_module_source(uri: &Uri, module: &str) -> Option<(Uri, String)> {
    let path = document_path(uri)?;
    let target = emet::manifest::search_path_for(&path)
        .directories()
        .iter()
        .map(|directory| directory.join(format!("{module}.emet")))
        .find(|candidate| candidate.exists())?;
    let target_source = std::fs::read_to_string(&target).ok()?;
    let target_uri: Uri = format!("file://{}", target.display()).parse().ok()?;
    Some((target_uri, target_source))
}

/// Convert an LSP `Position` (zero-based line, UTF-16 code-unit column) to a
/// byte offset into `source` — the index `emet`'s span-keyed queries expect.
/// LSP columns count UTF-16 units, not bytes, so the column walk advances by
/// each char's `len_utf16`. A position past the last line or column clamps to
/// the end of source. `position_at` is the inverse, for reporting spans back.
pub fn offset_at(source: &str, position: Position) -> usize {
    let mut line_start = 0usize;
    let mut line = 0u32;
    for (index, byte) in source.as_bytes().iter().enumerate() {
        if line == position.line {
            break;
        }
        if *byte == b'\n' {
            line += 1;
            line_start = index + 1;
        }
    }
    if line != position.line {
        return source.len();
    }
    let mut utf16 = 0u32;
    for (byte_index, ch) in source[line_start..].char_indices() {
        if ch == '\n' || utf16 >= position.character {
            return line_start + byte_index;
        }
        utf16 += ch.len_utf16() as u32;
    }
    source.len()
}

pub fn diagnostics_for(uri: &Uri, source: &str) -> Vec<Diagnostic> {
    analysis_of(uri, source)
        .diagnostics
        .iter()
        .map(|error| diagnostic_from_error(source, error))
        .collect()
}

fn diagnostic_from_error(source: &str, error: &emet::Error) -> Diagnostic {
    let message = match &error.note {
        Some(note) => format!("{:?}: {}\n{}", error.phase, error.msg, note),
        None => format!("{:?}: {}", error.phase, error.msg),
    };
    Diagnostic {
        range: span_to_range(source, &error.span),
        severity: Some(DiagnosticSeverity::ERROR),
        message,
        ..Default::default()
    }
}

pub fn span_to_range(source: &str, span: &std::ops::Range<usize>) -> Range {
    let start = position_at(source, span.start);
    let end = position_at(source, span.end.max(span.start));
    Range { start, end }
}

fn position_at(source: &str, byte_offset: usize) -> Position {
    let clamped = byte_offset.min(source.len());
    let mut line = 0u32;
    let mut line_start = 0usize;
    for (index, byte) in source.as_bytes()[..clamped].iter().enumerate() {
        if *byte == b'\n' {
            line += 1;
            line_start = index + 1;
        }
    }
    let character = source[line_start..clamped]
        .chars()
        .map(|c| c.len_utf16() as u32)
        .sum();
    Position { line, character }
}
