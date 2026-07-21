use std::path::PathBuf;

use lsp_types::{
    CompletionItem, CompletionItemKind, Diagnostic, DiagnosticSeverity, Hover, HoverContents,
    Location, MarkupContent, MarkupKind, Position, Range, Uri,
};

pub fn hover_at(source: &str, position: Position) -> Option<Hover> {
    let analysis = emet::analyze_source(source);
    let offset = offset_at(source, position);
    let ty = analysis.index.type_at(offset)?;
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!("```emet\n{ty}\n```"),
        }),
        range: None,
    })
}

pub fn completion_at(source: &str, position: Position) -> Vec<CompletionItem> {
    let analysis = emet::analyze_source(source);
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

pub fn definition_at(uri: &Uri, source: &str, position: Position) -> Option<Location> {
    let analysis = emet::analyze_source(source);
    let offset = offset_at(source, position);
    let site = analysis.index.definition_at(offset)?;
    match &site.module {
        None => Some(Location {
            uri: uri.clone(),
            range: span_to_range(source, &site.span),
        }),
        Some(module) => sibling_module_location(uri, module, &site.span),
    }
}

fn sibling_module_location(
    uri: &Uri,
    module: &str,
    span: &std::ops::Range<usize>,
) -> Option<Location> {
    let path = PathBuf::from(uri.as_str().strip_prefix("file://")?);
    let dir = path.parent()?;
    let target = dir.join(format!("{module}.emet"));
    let target_source = std::fs::read_to_string(&target).ok()?;
    let target_uri: Uri = format!("file://{}", target.display()).parse().ok()?;
    Some(Location {
        uri: target_uri,
        range: span_to_range(&target_source, span),
    })
}

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

pub fn diagnostics_for(source: &str) -> Vec<Diagnostic> {
    emet::analyze_source(source)
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
