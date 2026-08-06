use emet::compile_file;

fn fixtures_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/nested")
}

fn compile_fixture(entry: &str) -> Result<emet::Compiled, emet::Error> {
    compile_file(&fixtures_dir().join(entry))
}

fn scroll_names(c: &emet::Compiled) -> Vec<String> {
    c.scrolls.iter().map(|s| s.name.clone()).collect()
}

#[test]
fn a_dotted_import_loads_the_module_from_a_subdirectory() {
    let c = compile_fixture("NestedEntry.emet").expect("dotted import compiles");
    assert_eq!(scroll_names(&c), vec!["mysql".to_string()]);
}

#[test]
fn a_dotted_module_is_reachable_by_its_full_qualified_name() {
    let c = compile_fixture("NestedEntry.emet").expect("dotted import compiles");
    assert_eq!(
        c.scrolls[0].name, "mysql",
        "`Limesurvey.Database.containerName` names member `containerName` of module \
         `Limesurvey.Database`, not member `Database.containerName` of `Limesurvey`"
    );
}

#[test]
fn a_dotted_import_takes_an_alias() {
    let c = compile_fixture("AliasEntry.emet").expect("aliased dotted import compiles");
    assert_eq!(scroll_names(&c), vec!["mysql".to_string()]);
}

#[test]
fn a_dotted_import_exposes_names_unqualified() {
    let c = compile_fixture("ExposingEntry.emet").expect("exposing on a dotted import compiles");
    assert_eq!(scroll_names(&c), vec!["mysql".to_string()]);
}

#[test]
fn a_missing_dotted_module_names_the_nested_path_it_searched() {
    let err = compile_file(&fixtures_dir().join("MissingEntry.emet"))
        .expect_err("an absent nested module is an error");
    assert!(
        err.msg.contains("Limesurvey/Absent.emet"),
        "the search list should show the nested path, not a flat one; got: {}",
        err.msg
    );
}

#[test]
fn a_header_that_disagrees_with_its_path_is_an_error_naming_both() {
    let err = compile_file(&fixtures_dir().join("MismatchEntry.emet"))
        .expect_err("a module whose header disagrees with its location is an error");
    assert!(
        err.msg.contains("Mismatch.Named") && err.msg.contains("Mismatch.Wrong"),
        "the diagnostic should name the imported module and what the file declares; got: {}",
        err.msg
    );
}

/// The point of ADR 0049: a module split by concern gives both halves the same
/// obvious type name, and qualified identity is what lets them coexist.
#[test]
fn two_submodules_may_each_declare_a_type_of_the_same_name() {
    let c = compile_file(&fixtures_dir().join("SplitEntry.emet"))
        .expect("two submodules may each declare a `Config`");
    assert_eq!(
        scroll_names(&c),
        vec!["mysql".to_string(), "limesurvey".to_string()]
    );
}

#[test]
fn a_bare_type_name_two_imports_both_expose_is_ambiguous() {
    let err = compile_file(&fixtures_dir().join("AmbiguousEntry.emet"))
        .expect_err("a bare `Thing` with two candidates has no single meaning");
    assert!(
        err.msg.contains("`Thing` is ambiguous"),
        "the error names the ambiguous type; got: {}",
        err.msg
    );
    let note = err.note.unwrap_or_default();
    assert!(
        note.contains("Amb.A.Thing"),
        "the note offers the qualified spelling to escape into; got: {note}"
    );
}

#[test]
fn a_qualified_annotation_picks_one_of_two_same_named_types() {
    let c = compile_file(&fixtures_dir().join("QualifiedAnnotationEntry.emet"))
        .expect("naming the type in full resolves the ambiguity");
    assert_eq!(scroll_names(&c), vec!["x".to_string()]);
}
