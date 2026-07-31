use emet::analyze_source;

const SHAPES: &str = "type Shape = Circle Int | Square Int\n\ntype Wrapper a = Wrap (List a)\n\nmain : List Scroll\nmain =\n  []\n";

#[test]
fn a_local_type_declaration_is_rendered_with_its_constructors() {
    let analysis = analyze_source(SHAPES);
    let definition = analysis
        .index
        .type_definitions
        .get("Shape")
        .expect("Shape is defined in this module");
    assert_eq!(
        definition.declaration,
        "type Shape\n    = Circle Int\n    | Square Int"
    );
    assert!(definition.site.module.is_none());
}

#[test]
fn a_constructor_field_that_takes_arguments_is_parenthesized() {
    let analysis = analyze_source(SHAPES);
    let definition = analysis.index.type_definitions.get("Wrapper").unwrap();
    assert_eq!(
        definition.declaration,
        "type Wrapper a\n    = Wrap (List a)"
    );
}

#[test]
fn the_type_name_under_the_cursor_is_the_one_written_there() {
    let offset = SHAPES.find("List Scroll").unwrap() + "List ".len();
    assert_eq!(
        emet::query::type_name_at(SHAPES, offset).as_deref(),
        Some("Scroll")
    );
    let lowercase = SHAPES.find("main =").unwrap();
    assert_eq!(emet::query::type_name_at(SHAPES, lowercase), None);
}

#[test]
fn a_builtin_sum_type_is_rendered_from_the_preludes_own_constructors() {
    let glyph = emet::query::builtin_type_declaration("Glyph").expect("Glyph is a builtin type");
    assert!(glyph.starts_with("type Glyph\n"), "{glyph}");
    for constructor in ["AptPackage", "SystemdService", "Filesystem", "LineInFile"] {
        assert!(glyph.contains(constructor), "{glyph}");
    }
}

#[test]
fn a_scroll_of_the_documented_builtins_renders_its_authoring_shape() {
    let scroll =
        emet::query::builtin_type_declaration("Scroll").expect("Scroll is a documented builtin");
    assert!(scroll.starts_with("type Scroll\nscroll\n"), "{scroll}");
    for field in ["name : String", "policy : Policy", "notifies : List String"] {
        assert!(scroll.contains(field), "{scroll}");
    }
    let doc = emet::query::builtin_type_doc("Scroll").expect("Scroll carries prose");
    assert!(doc.contains("notifies"), "{doc}");
}

#[test]
fn an_undocumented_builtin_renders_as_its_name_and_carries_no_prose() {
    assert_eq!(
        emet::query::builtin_type_declaration("String").as_deref(),
        Some("type String")
    );
    assert_eq!(emet::query::builtin_type_doc("String"), None);
    assert_eq!(emet::query::builtin_type_declaration("Nonesuch"), None);
}
