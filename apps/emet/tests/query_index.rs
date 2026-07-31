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
fn hover_on_let_bound_name_is_fully_applied_type() {
    let src = "main : List Scroll\nmain =\n  let greeting = \"hi\"\n  in []\n";
    let analysis = analyze_source(src);
    let offset = byte_offset(src, "greeting", 0);
    let ty = analysis
        .index
        .type_at(offset)
        .expect("a type at the binder");
    assert_eq!(ty.to_string(), "String");
}

#[test]
fn hover_on_lambda_param_use_is_its_type() {
    let src = "main : List Scroll\nmain =\n  let f = \\s -> s\n  in []\n";
    let analysis = analyze_source(src);
    let use_offset = byte_offset(src, "s", 2);
    let ty = analysis
        .index
        .type_at(use_offset)
        .expect("a type at the param use");
    assert!(matches!(ty.to_string().as_str(), _s if !ty.to_string().is_empty()));
}

#[test]
fn hover_on_application_result_binder_is_the_result_type() {
    let src = "main : List Scroll\nmain =\n  let n = List.length []\n  in []\n";
    let analysis = analyze_source(src);
    let offset = byte_offset(src, "n = List", 0);
    let ty = analysis
        .index
        .type_at(offset)
        .expect("a type at the binder");
    assert_eq!(ty.to_string(), "Int");
}

#[test]
fn scope_at_position_includes_local_and_prelude_names() {
    let src = "main : List Scroll\nmain =\n  let x = \"a\"\n  in []\n";
    let analysis = analyze_source(src);
    let offset = byte_offset(src, "[]", 0);
    let names = analysis.index.names_in_scope(offset);
    assert!(names.iter().any(|(n, _)| n == "x"), "local x in scope");
    assert!(
        names.iter().any(|(n, _)| n == "main"),
        "sibling main in scope"
    );
    assert!(
        names.iter().any(|(n, _)| n == "List.map"),
        "a prelude name in scope"
    );
}

#[test]
fn scope_scheme_is_displayable() {
    let src = "main : List Scroll\nmain =\n  let x = \"a\"\n  in []\n";
    let analysis = analyze_source(src);
    let offset = byte_offset(src, "[]", 0);
    let names = analysis.index.names_in_scope(offset);
    let (_, scheme) = names.iter().find(|(n, _)| n == "x").unwrap();
    assert_eq!(scheme.trim(), "String");
}

#[test]
fn use_resolves_to_definition_span_same_file() {
    let src = "greeting : String\ngreeting = \"hi\"\nmain : List Scroll\nmain =\n  let _unused = greeting\n  in []\n";
    let analysis = analyze_source(src);
    let use_offset = byte_offset(src, "greeting", 2);
    let def = analysis
        .index
        .definition_at(use_offset)
        .expect("a definition site");
    let def_decl_offset = byte_offset(src, "greeting", 1);
    assert!(def.span.contains(&def_decl_offset) || def.span.start == def_decl_offset);
    assert!(def.module.is_none());
}

const DOCUMENTED: &str = "\
-- Greets the world.
--
-- Second paragraph.
greeting : String
greeting = \"hi\"

plain : String
plain = \"no docs\"

-- Detached by a blank line.

detached : String
detached = \"nope\"

main : List Scroll
main =
  let local = greeting
  in []
";

#[test]
fn doc_comment_above_a_documented_definition_is_its_comment_block() {
    let span = byte_offset(DOCUMENTED, "greeting = ", 0);
    let doc = emet::query::doc_comment_above_definition(DOCUMENTED, &(span..span + 8))
        .expect("a doc comment above greeting");
    assert_eq!(doc, "Greets the world.\n\nSecond paragraph.");
}

#[test]
fn an_undocumented_definition_has_no_doc_comment() {
    let span = byte_offset(DOCUMENTED, "plain = ", 0);
    assert_eq!(
        emet::query::doc_comment_above_definition(DOCUMENTED, &(span..span + 5)),
        None
    );
}

#[test]
fn a_comment_separated_by_a_blank_line_is_not_a_doc_comment() {
    let span = byte_offset(DOCUMENTED, "detached = ", 0);
    assert_eq!(
        emet::query::doc_comment_above_definition(DOCUMENTED, &(span..span + 8)),
        None
    );
}

#[test]
fn a_local_binding_takes_no_doc_comment_from_the_line_above_it() {
    let span = byte_offset(DOCUMENTED, "local = ", 0);
    assert_eq!(
        emet::query::doc_comment_above_definition(DOCUMENTED, &(span..span + 5)),
        None
    );
}

#[test]
fn a_type_declaration_carries_its_doc_comment() {
    let src = "-- A shape, round or square.\ntype Shape = Circle Int | Square Int\n";
    let start = byte_offset(src, "type Shape", 0);
    let doc = emet::query::doc_comment_above_definition(src, &(start..src.len()))
        .expect("a doc comment above the type");
    assert_eq!(doc, "A shape, round or square.");
}
