//! The LSP feature layer (ADR 0018): translate an editor request at a cursor
//! into an `emet` query and shape the answer for LSP. It holds **no language
//! semantics** — every type, scope, and definition comes from `emet`'s single
//! inference engine via `analyze_source`, so the adapter and the compiler can
//! never disagree. What lives here is purely the LSP boundary: LSP UTF-16
//! positions ↔ byte offsets, and `emet` types ↔ LSP payloads.

use std::path::PathBuf;

use lsp_types::{
    CompletionItem, CompletionItemKind, Diagnostic, DiagnosticSeverity, Hover, HoverContents,
    Location, MarkupContent, MarkupKind, Position, Range, Uri,
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

/// The inferred type at the cursor, as a Markdown hover. `None` when no
/// recorded expression covers the position.
pub fn hover_at(uri: &Uri, source: &str, position: Position) -> Option<Hover> {
    let analysis = analysis_of(uri, source);
    let offset = offset_at(source, position);
    let ty = analysis.index.type_at(offset)?;
    let described = analysis
        .index
        .definition_at(offset)
        .map(|site| describe_definition(uri, source, site))
        .unwrap_or_default();
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: hover_markdown(&ty.to_string(), described),
        }),
        range: None,
    })
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
