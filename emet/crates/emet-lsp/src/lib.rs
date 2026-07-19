use lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};

pub fn diagnostics_for(source: &str) -> Vec<Diagnostic> {
    match emet::compile(source) {
        Ok(_) => Vec::new(),
        Err(error) => vec![diagnostic_from_error(source, &error)],
    }
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
