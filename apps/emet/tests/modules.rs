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

fn assert_undeclared_exposure(err: &emet::Error, module: &str, name: &str) {
    assert_eq!(err.phase, Phase::Type);
    let rendered = format!("{} {}", err.msg, err.note.clone().unwrap_or_default());
    for needle in [format!("`{module}`"), format!("`{name}`")] {
        assert!(
            rendered.contains(&needle),
            "the diagnostic must contain {needle}, got: {rendered}"
        );
    }
    assert!(
        rendered.contains("declare"),
        "the diagnostic must say the name has to be declared here, got: {rendered}"
    );
}

// Before ADR 0049 the two halves of a re-export diverged, and neither was an
// error: the value case compiled and handed the importer the declaring module's
// value through the relay, while the type case dropped out of the interface
// silently and surfaced later as "does not expose". Both are the exposing list's
// error now.
#[test]
fn a_module_may_not_expose_a_value_it_did_not_declare() {
    let err = compile_fixture("ReexportValueEntry.emet")
        .expect_err("re-exposing an imported value must not compile");
    assert_undeclared_exposure(&err, "ReexportValue", "thing");
}

#[test]
fn a_module_may_not_expose_a_type_it_did_not_declare() {
    let err = compile_fixture("ReexportTypeEntry.emet")
        .expect_err("re-exposing an imported type must not compile");
    assert_undeclared_exposure(&err, "ReexportType", "Tag");
}

#[test]
fn exposing_a_constructor_by_itself_points_at_its_type() {
    let err = compile_fixture("ExposeCtorEntry.emet")
        .expect_err("a constructor is not a type, so `exposing (Wrap)` must not compile");
    assert_eq!(err.phase, Phase::Type);
    let rendered = format!("{} {}", err.msg, err.note.clone().unwrap_or_default());
    assert!(
        rendered.contains("`Wrap`") && rendered.contains("`ExposeCtor`"),
        "the diagnostic must name the constructor and the module exposing it, got: {rendered}"
    );
    assert!(
        err.note.unwrap_or_default().contains("`Tag(..)`"),
        "the note must point at the type whose constructors the author wants"
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

fn assert_ambiguous_constructor(err: &emet::Error, name: &str, first: &str, second: &str) {
    assert_eq!(err.phase, Phase::Type);
    let rendered = format!("{} {}", err.msg, err.note.clone().unwrap_or_default());
    assert!(
        err.msg.contains(&format!("`{name}` is ambiguous")),
        "the message must name the ambiguous constructor, got: {rendered}"
    );
    for needle in [format!("`{first}.{name}`"), format!("`{second}.{name}`")] {
        assert!(
            rendered.contains(&needle),
            "the diagnostic must offer {needle} as a spelling, got: {rendered}"
        );
    }
}

#[test]
fn same_named_types_from_two_imports_stay_distinct() {
    let err = compile_fixture("ShadowCaseEntry.emet")
        .expect_err("two imports' `Thing`s are two types, so mixing them is a mismatch");
    assert_eq!(err.phase, Phase::Type);
    assert!(
        err.msg.contains("`ShadowSingle.Thing`") && err.msg.contains("`ShadowMulti.Thing`"),
        "a mismatch between two same-named types has to qualify both, got: {}",
        err.msg
    );
}

#[test]
fn same_named_types_from_two_imports_stay_distinct_at_the_argument_gate() {
    let err = compile_fixture("ShadowParamEntry.emet")
        .expect_err("destructuring a multi-constructor type in an argument is still refused");
    let rendered = format!("{} {}", err.msg, err.note.clone().unwrap_or_default());
    assert!(
        rendered.contains("`Thing`"),
        "the argument-pattern message names the type as written, got: {rendered}"
    );
}

#[test]
fn a_local_type_and_an_imported_one_of_the_same_name_stay_distinct() {
    let err = compile_fixture("ShadowLocalEntry.emet")
        .expect_err("a local `Thing` is not the imported `Thing`, so mixing them is a mismatch");
    assert_eq!(err.phase, Phase::Type);
    assert!(
        err.msg.contains("`Main.Thing`") && err.msg.contains("`ShadowMulti.Thing`"),
        "a mismatch between two same-named types has to qualify both, got: {}",
        err.msg
    );
}

#[test]
fn same_named_opaque_types_from_two_imports_stay_distinct() {
    let err = compile_fixture("ShadowOpaqueEntry.emet")
        .expect_err("two private `Thing`s are two types, so passing one for the other is an error");
    assert_eq!(err.phase, Phase::Type);
    assert!(
        err.msg.contains("`ShadowOpaqueA.Thing`") && err.msg.contains("`ShadowOpaqueB.Thing`"),
        "a mismatch between two same-named types has to qualify both, got: {}",
        err.msg
    );
}

#[test]
fn two_imports_sharing_a_third_modules_type_still_compile() {
    let c = compile_fixture("SharedEntry.emet")
        .expect("one type reached through two imports is not a collision");
    assert_eq!(scroll_names(&c), vec!["shared".to_string()]);
}

fn assert_reports_no_type_mismatch(err: &emet::Error) {
    let rendered = format!("{} {}", err.msg, err.note.clone().unwrap_or_default());
    assert!(
        !rendered.contains("type mismatch"),
        "the collision must be reported directly, not as a mismatch against the surviving constructor's type, got: {rendered}"
    );
}

#[test]
fn a_bare_constructor_two_imports_both_define_is_ambiguous() {
    let err = compile_fixture("CtorCollisionEntry.emet")
        .expect_err("a bare `Wrap` with two candidates has no single meaning");
    assert_ambiguous_constructor(&err, "Wrap", "CtorA", "CtorB");
    assert_reports_no_type_mismatch(&err);
}

#[test]
fn a_bare_constructor_is_ambiguous_on_the_pattern_side_too() {
    let err = compile_fixture("CtorCollisionMatchEntry.emet")
        .expect_err("a bare `Wrap` with two candidates has no single meaning");
    assert_ambiguous_constructor(&err, "Wrap", "CtorA", "CtorB");
    assert_reports_no_type_mismatch(&err);
}

#[test]
fn a_bare_constructor_a_local_type_and_an_import_both_define_is_ambiguous() {
    let err = compile_fixture("CtorCollisionLocalEntry.emet")
        .expect_err("a bare `Wrap` naming both a local and an imported constructor is ambiguous");
    assert_ambiguous_constructor(&err, "Wrap", "Main", "CtorA");
    assert_reports_no_type_mismatch(&err);
}

#[test]
fn an_ambiguous_constructor_is_reported_at_the_reference_not_at_the_import() {
    let err = compile_fixture("CtorCollisionEntry.emet")
        .expect_err("a bare `Wrap` with two candidates has no single meaning");
    let source = std::fs::read_to_string(fixtures_dir().join("CtorCollisionEntry.emet")).unwrap();
    assert_eq!(
        &source[err.span.clone()],
        "Wrap",
        "the span must underline the reference, not the `import` line"
    );
}

#[test]
fn a_qualified_constructor_its_module_does_not_have_names_both() {
    let err = compile_fixture("CtorMisspelledEntry.emet")
        .expect_err("`CtorA.Wrup` is not a constructor `CtorA` puts in scope");
    let rendered = format!("{} {}", err.msg, err.note.clone().unwrap_or_default());
    assert!(
        err.msg.contains("`CtorA`") && err.msg.contains("`Wrup`"),
        "the message must name the module and the constructor, got: {rendered}"
    );
}

#[test]
fn a_qualified_constructor_picks_one_of_two_same_named_ones() {
    let c = compile_fixture("CtorQualifiedEntry.emet")
        .expect("naming the constructor in full resolves the ambiguity");
    assert_eq!(scroll_names(&c), vec!["text".to_string()]);
}

#[test]
fn a_qualified_constructor_works_in_pattern_position() {
    let c = compile_fixture("CtorQualifiedMatchEntry.emet")
        .expect("a qualified constructor matches as well as it builds");
    assert_eq!(scroll_names(&c), vec!["matched".to_string()]);
}

#[test]
fn a_qualified_constructor_honors_the_import_alias() {
    let c = compile_fixture("CtorQualifiedAliasEntry.emet")
        .expect("`import CtorA as A` makes `A.Wrap` the qualified spelling");
    assert_eq!(scroll_names(&c), vec!["aliased".to_string()]);
}

#[test]
fn a_local_constructor_has_a_qualified_spelling_of_its_own() {
    let c = compile_fixture("CtorQualifiedLocalEntry.emet")
        .expect("`Main.Wrap` names this module's own constructor");
    assert_eq!(scroll_names(&c), vec!["text".to_string()]);
}

#[test]
fn exhaustiveness_still_sees_a_qualified_pattern_as_covering_its_constructor() {
    let c = compile_fixture("CtorQualifiedCoverEntry.emet")
        .expect("two qualified arms cover a two-constructor type");
    assert_eq!(scroll_names(&c), vec!["plain".to_string()]);
}

#[test]
fn a_qualified_case_missing_an_arm_is_still_non_exhaustive() {
    let err = compile_fixture("CtorQualifiedExhaustEntry.emet")
        .expect_err("one qualified arm does not cover a two-constructor type");
    let rendered = format!("{} {}", err.msg, err.note.clone().unwrap_or_default());
    assert!(
        rendered.contains("non-exhaustive") && rendered.contains("Plain"),
        "the checker must still name the missing constructor, got: {rendered}"
    );
}

#[test]
fn one_module_imported_twice_does_not_collide_with_itself() {
    let c = compile_fixture("CtorTwiceEntry.emet")
        .expect("one constructor reached through two imports of one module is not a collision");
    assert_eq!(scroll_names(&c), vec!["twice".to_string()]);
}

#[test]
fn a_constructor_behind_a_closed_type_export_does_not_collide() {
    let c = compile_fixture("CtorHiddenEntry.emet")
        .expect("a constructor an import never exposes cannot collide with a local one");
    assert_eq!(scroll_names(&c), vec!["hidden".to_string()]);
}
