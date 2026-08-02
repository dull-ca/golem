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
    assert_eq!(c.scrolls[0].glyphs().len(), 1);
    assert_eq!(c.scrolls[0].glyphs()[0].key(), "apt:nginx");
}

#[test]
fn import_alias_access_resolves() {
    let c = compile_fixture("AliasEntry.emet").expect("aliased import compiles");
    assert_eq!(c.scrolls[0].glyphs()[0].key(), "apt:nginx");
}

#[test]
fn import_exposing_brings_name_into_scope_unqualified() {
    let c = compile_fixture("ExposingEntry.emet").expect("import exposing compiles");
    assert_eq!(c.scrolls[0].glyphs()[0].key(), "apt:nginx");
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
    assert_eq!(c.scrolls[0].glyphs()[0].key(), "apt:nginx");
    assert_eq!(c.scrolls[1].glyphs()[0].key(), "apt:postgresql");
}

#[test]
fn imported_open_type_is_usable_in_annotation() {
    let c = compile_fixture("AnnotEntry.emet").expect("imported Type(..) annotation compiles");
    assert_eq!(scroll_names(&c), vec!["web".to_string()]);
    assert_eq!(c.scrolls[0].glyphs()[0].key(), "apt:nginx");
}

#[test]
fn imported_open_type_constructors_are_matchable() {
    let c = compile_fixture("MatchEntry.emet").expect("case on imported Role(..) compiles");
    assert_eq!(scroll_names(&c), vec!["web".to_string(), "db".to_string()]);
}

#[test]
fn imported_open_type_exhaustiveness_crosses_module() {
    let err = compile_fixture("MatchNonExhaustiveEntry.emet")
        .expect_err("a missing arm on an imported type must be non-exhaustive");
    assert_eq!(err.phase, Phase::Type);
    assert!(
        err.msg.contains("non-exhaustive"),
        "expected a non-exhaustive diagnostic, got: {}",
        err.msg
    );
}

#[test]
fn parameterized_open_type_crosses_module() {
    // An arity-1 `Box a` exported `Box(..)`: the importer annotates with it
    // (`Box String`, `Box a -> a`), constructs it, and matches it — every path
    // that depends on the imported type's arity being carried across the boundary.
    let c = compile_fixture("BoxesEntry.emet").expect("imported parameterized Box(..) compiles");
    assert_eq!(scroll_names(&c), vec!["boxed".to_string()]);
    assert_eq!(
        c.scrolls[0]
            .glyphs()
            .iter()
            .map(|g| g.key())
            .collect::<Vec<_>>(),
        vec![
            "systemd:boxed.service".to_string(),
            "systemd:matched.service".to_string()
        ],
    );
}

#[test]
fn type_imported_without_open_is_not_matchable() {
    let err = compile_fixture("OpaqueMatchEntry.emet")
        .expect_err("matching constructors of a type imported without (..) must fail");
    assert_eq!(err.phase, Phase::Type);
    assert!(
        err.msg.contains("unknown constructor"),
        "expected an unknown-constructor diagnostic, got: {}",
        err.msg
    );
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
    assert_eq!(c.scrolls[0].glyphs()[0].key(), "apt:nginx");
    assert_eq!(c.scrolls[1].glyphs()[0].key(), "apt:postgresql");
}

#[test]
fn imported_single_constructor_type_destructures_in_an_argument() {
    let c = compile_fixture("DestructureEntry.emet")
        .expect("an open-exposed single-constructor type destructures in a parameter");
    assert_eq!(scroll_names(&c), vec!["unwrapped".to_string()]);
}

#[test]
fn imported_multi_constructor_type_may_not_destructure_in_an_argument() {
    let err = compile_fixture("DestructureMultiEntry.emet")
        .expect_err("an open-exposed multi-constructor type stays `case`-only");
    assert_eq!(err.phase, Phase::Type);
    assert!(
        err.msg.contains("`Web`") && err.msg.contains("`Role`"),
        "the message must name the imported constructor and its type, got: {}",
        err.msg
    );
    assert!(
        err.note.unwrap_or_default().contains("case"),
        "the note must direct the author to `case`"
    );
}

fn assert_names_both_modules(err: &emet::Error, type_name: &str, first: &str, second: &str) {
    assert_eq!(err.phase, Phase::Type);
    let rendered = format!("{} {}", err.msg, err.note.clone().unwrap_or_default());
    for needle in [
        format!("`{type_name}`"),
        format!("`{first}`"),
        format!("`{second}`"),
    ] {
        assert!(
            rendered.contains(&needle),
            "the diagnostic must contain {needle}, got: {rendered}"
        );
    }
    assert!(
        err.note
            .clone()
            .unwrap_or_default()
            .to_lowercase()
            .contains("rename"),
        "the note must tell the author what to do, got: {:?}",
        err.note
    );
}

#[test]
fn same_named_types_from_two_imports_are_rejected() {
    let err = compile_fixture("ShadowCaseEntry.emet")
        .expect_err("two imports defining `Thing` must not compile");
    assert_names_both_modules(&err, "Thing", "ShadowMulti", "ShadowSingle");
}

#[test]
fn same_named_types_are_rejected_before_the_argument_pattern_gate() {
    let err = compile_fixture("ShadowParamEntry.emet")
        .expect_err("two imports defining `Thing` must not compile");
    assert_names_both_modules(&err, "Thing", "ShadowMulti", "ShadowSingle");
}

#[test]
fn a_local_type_may_not_share_a_name_with_an_imported_one() {
    let err = compile_fixture("ShadowLocalEntry.emet")
        .expect_err("a local `Thing` must not collide with an imported `Thing`");
    assert_names_both_modules(&err, "Thing", "Main", "ShadowMulti");
}

#[test]
fn same_named_opaque_types_from_two_imports_are_rejected() {
    let err = compile_fixture("ShadowOpaqueEntry.emet")
        .expect_err("two imports whose signatures mention a private `Thing` must not compile");
    assert_names_both_modules(&err, "Thing", "ShadowOpaqueA", "ShadowOpaqueB");
}

#[test]
fn two_imports_sharing_a_third_modules_type_still_compile() {
    let c = compile_fixture("SharedEntry.emet")
        .expect("one type reached through two imports is not a collision");
    assert_eq!(scroll_names(&c), vec!["shared".to_string()]);
}
