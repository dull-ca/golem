use emet_lsp::{diagnostics_for, span_to_range};
use lsp_types::{DiagnosticSeverity, Position, Uri};

fn scratch_uri() -> Uri {
    "untitled:scratch.emet".parse().unwrap()
}

const VALID: &str = "main : List Scroll\nmain =\n  []\n";
const BROKEN_LINE_THREE: &str = "main : List Scroll\nmain =\n  undefinedThing\n";

#[test]
fn valid_program_yields_no_diagnostics() {
    assert!(diagnostics_for(&scratch_uri(), VALID).is_empty());
}

#[test]
fn broken_program_yields_one_error_diagnostic() {
    let diagnostics = diagnostics_for(&scratch_uri(), BROKEN_LINE_THREE);
    assert_eq!(diagnostics.len(), 1);
    let diagnostic = &diagnostics[0];
    assert_eq!(diagnostic.severity, Some(DiagnosticSeverity::ERROR));
    assert!(diagnostic
        .message
        .starts_with("Type: unknown name `undefinedThing`"));
    assert!(diagnostic.message.contains("not bound by any declaration"));
}

#[test]
fn error_on_line_three_maps_to_line_three() {
    let diagnostic = &diagnostics_for(&scratch_uri(), BROKEN_LINE_THREE)[0];
    assert_eq!(diagnostic.range.start, Position::new(2, 2));
    assert_eq!(diagnostic.range.end, Position::new(2, 16));
}

#[test]
fn utf16_characters_advance_the_column() {
    let source = "héllo world";
    let space = source.find(' ').unwrap();
    let range = span_to_range(source, &(space..space));
    assert_eq!(range.start, Position::new(0, 5));
}

#[test]
fn astral_plane_char_counts_as_two_utf16_units() {
    let source = "x = 😀 more";
    let m = source.find('m').unwrap();
    let range = span_to_range(source, &(m..m));
    assert_eq!(range.start, Position::new(0, 7));
}

#[test]
fn empty_span_points_at_document_start() {
    let range = span_to_range("anything", &(0..0));
    assert_eq!(range.start, Position::new(0, 0));
    assert_eq!(range.end, Position::new(0, 0));
}
