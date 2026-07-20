mod common;

use emet::{compile, compile_file, Phase};

fn fixtures_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/modules")
}

fn compile_fixture(entry: &str) -> Result<emet::Compiled, emet::Error> {
    compile_file(&fixtures_dir().join(entry))
}

fn scroll_names(c: &emet::Compiled) -> Vec<String> {
    c.scrolls.iter().map(|s| s.name.clone()).collect()
}

#[test]
fn header_less_single_file_still_compiles() {
    let src = r#"
main : List Scroll
main = [ scroll { name = "solo", glyphs = [ aptPackage { name = "nginx" } ] } ]
"#;
    let c = compile(src).expect("header-less file compiles");
    assert_eq!(c.scrolls.len(), 1);
    assert_eq!(c.scrolls[0].name, "solo");
}

#[test]
fn module_header_with_exposing_all_compiles() {
    let src = r#"module Main exposing (..)

main : List Scroll
main = [ scroll { name = "solo", glyphs = [ aptPackage { name = "nginx" } ] } ]
"#;
    let c = compile(src).expect("module header compiles as a single file");
    assert_eq!(c.scrolls.len(), 1);
    assert_eq!(c.scrolls[0].name, "solo");
}

#[test]
fn import_qualified_access_resolves() {
    let c = compile_fixture("QualifiedEntry.emet").expect("qualified import compiles");
    assert_eq!(scroll_names(&c), vec!["web".to_string()]);
    assert_eq!(c.scrolls[0].glyphs.len(), 1);
    assert_eq!(c.scrolls[0].glyphs[0].key(), "apt:nginx");
}

#[test]
fn import_alias_access_resolves() {
    let c = compile_fixture("AliasEntry.emet").expect("aliased import compiles");
    assert_eq!(c.scrolls[0].glyphs[0].key(), "apt:nginx");
}

#[test]
fn import_exposing_brings_name_into_scope_unqualified() {
    let c = compile_fixture("ExposingEntry.emet").expect("import exposing compiles");
    assert_eq!(c.scrolls[0].glyphs[0].key(), "apt:nginx");
}

#[test]
fn non_exposed_decl_is_not_importable() {
    let err = compile_fixture("HiddenEntry.emet").expect_err("hidden decl must not resolve");
    assert_eq!(err.phase, Phase::Type);
}

#[test]
fn exposing_a_non_exposed_name_is_rejected() {
    let err = compile_fixture("ExposingHiddenEntry.emet")
        .expect_err("importing a non-exposed name must fail");
    assert_eq!(err.phase, Phase::Type);
    assert!(
        err.msg.contains("does not expose"),
        "expected a not-exposed diagnostic, got: {}",
        err.msg
    );
}

#[test]
fn exposed_type_constructors_cross_module() {
    let c = compile_fixture("RolesEntry.emet").expect("Type(..) exposure compiles");
    assert_eq!(scroll_names(&c), vec!["web".to_string(), "db".to_string()]);
    assert_eq!(c.scrolls[0].glyphs[0].key(), "apt:nginx");
    assert_eq!(c.scrolls[1].glyphs[0].key(), "apt:postgresql");
}

#[test]
fn imported_open_type_is_usable_in_annotation() {
    let c = compile_fixture("AnnotEntry.emet").expect("imported Type(..) annotation compiles");
    assert_eq!(scroll_names(&c), vec!["web".to_string()]);
    assert_eq!(c.scrolls[0].glyphs[0].key(), "apt:nginx");
}

#[test]
fn annotation_with_non_exposed_type_is_rejected() {
    let err = compile_fixture("HiddenTypeAnnotEntry.emet")
        .expect_err("annotating with a non-exposed type must fail");
    assert_eq!(err.phase, Phase::Type);
    assert!(
        err.msg.contains("unknown type constructor"),
        "expected an unknown-type-constructor diagnostic, got: {}",
        err.msg
    );
}

#[test]
fn import_cycle_is_rejected() {
    let err = compile_fixture("CycleA.emet").expect_err("import cycle must be rejected");
    assert!(
        err.msg.contains("cycle"),
        "expected a cycle diagnostic, got: {}",
        err.msg
    );
}

#[test]
fn missing_main_is_rejected() {
    let err = compile_fixture("NoMainEntry.emet").expect_err("entry without main must fail");
    assert_eq!(err.phase, Phase::Type);
}

#[test]
fn library_module_may_not_have_main() {
    let err = compile_fixture("TwoMainEntry.emet").expect_err("a library with main must fail");
    assert_eq!(err.phase, Phase::Type);
}

#[test]
fn multi_module_program_compiles_to_expected_scrolls() {
    let c = compile_fixture("FleetEntry.emet").expect("multi-module program compiles");
    assert_eq!(scroll_names(&c), vec!["web".to_string(), "db".to_string()]);
    assert_eq!(c.scrolls[0].glyphs[0].key(), "apt:nginx");
    assert_eq!(c.scrolls[1].glyphs[0].key(), "apt:postgresql");
}
