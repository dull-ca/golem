use emet::analyze_source;

fn byte_offset(src: &str, needle: &str, occurrence: usize) -> usize {
    let mut start = 0;
    for _ in 0..occurrence {
        let found = src[start..].find(needle).expect("needle present");
        start += found + needle.len();
    }
    let found = src[start..].find(needle).expect("needle present");
    start + found
}

#[test]
fn the_outline_lists_top_level_definitions_with_their_types() {
    let src = "module Shapes exposing (..)\n\ntype Shape = Circle Int | Square Int\n\narea : Shape -> Int\narea s =\n  case s of\n    Circle r ->\n      r\n    Square w ->\n      w\n\nunit : Int\nunit = 1\n";
    let analysis = analyze_source(src);
    let outline = emet::document_outline(src, &analysis.index);

    let names: Vec<&str> = outline.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["Shapes", "Shape", "area", "unit"]);

    let area = outline.iter().find(|s| s.name == "area").unwrap();
    assert_eq!(area.kind, emet::query::SymbolKind::Function);
    assert_eq!(area.detail.as_deref(), Some("Shape -> Int"));
    assert_eq!(area.name_span.start, byte_offset(src, "area s =", 0));

    let unit = outline.iter().find(|s| s.name == "unit").unwrap();
    assert_eq!(unit.kind, emet::query::SymbolKind::Value);
    assert_eq!(unit.detail.as_deref(), Some("Int"));

    let shape = outline.iter().find(|s| s.name == "Shape").unwrap();
    assert_eq!(shape.kind, emet::query::SymbolKind::Type);
    let variants: Vec<&str> = shape.children.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(variants, vec!["Circle", "Square"]);
    assert_eq!(shape.children[0].detail.as_deref(), Some("Int"));
}

#[test]
fn outline_spans_enclose_their_selection_spans() {
    let src = "module Shapes exposing (..)\n\ntype Shape = Circle Int\n\narea : Shape -> Int\narea s =\n  case s of\n    Circle r ->\n      r\n";
    let analysis = analyze_source(src);
    for symbol in emet::document_outline(src, &analysis.index) {
        assert!(
            symbol.span.start <= symbol.name_span.start && symbol.name_span.end <= symbol.span.end,
            "{} selection {:?} outside range {:?}",
            symbol.name,
            symbol.name_span,
            symbol.span
        );
        assert!(src.get(symbol.name_span.clone()) == Some(symbol.name.as_str()));
    }
}
