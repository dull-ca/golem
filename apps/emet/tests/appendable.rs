//! Full `appendable` `++` (ADR 0007): `++ : appendable -> appendable ->
//! appendable`, satisfied by `String` and `List a` only. `++` dispatches at
//! eval time to string or list concatenation on the runtime value, so the same
//! operator joins strings and lists while rejecting every other type and any
//! String/List cross.

mod common;

use common::{err_phase, single_scroll_glyphs};
use emet::ir::Glyph;

fn unit(src: &str) -> String {
    match single_scroll_glyphs(src).as_slice() {
        [Glyph::SystemdService { unit }] => unit.clone(),
        other => panic!("expected a single systemdService glyph, got {other:?}"),
    }
}

#[test]
fn append_operator_chains_strings() {
    let src = r#"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = "a" ++ "b" ++ "c" } ] } ]
"#;
    assert_eq!(unit(src), "abc");
}

#[test]
fn append_operator_concatenates_glyph_lists() {
    let src = r#"
main = [ scroll { name = "test", glyphs = [ aptPackage { name = "a" } ] ++ [ aptPackage { name = "b" } ] } ]
"#;
    let rs = single_scroll_glyphs(src);
    assert_eq!(
        rs,
        vec![
            Glyph::AptPackage { name: "a".into() },
            Glyph::AptPackage { name: "b".into() },
        ]
    );
}

#[test]
fn append_operator_concatenates_string_lists() {
    let src = r#"
names = [ "x" ] ++ [ "y" ]
main = [ scroll { name = "test", glyphs = List.map (\n -> aptPackage { name = n }) names } ]
"#;
    let rs = single_scroll_glyphs(src);
    assert_eq!(
        rs,
        vec![
            Glyph::AptPackage { name: "x".into() },
            Glyph::AptPackage { name: "y".into() },
        ]
    );
}

#[test]
fn polymorphic_append_helper_serves_both_strings_and_lists() {
    let src = r#"
twice : appendable -> appendable
twice x = x ++ x
main = [ scroll { name = "test", glyphs = List.map (\n -> aptPackage { name = twice n }) (twice [ "a" ]) } ]
"#;
    let rs = single_scroll_glyphs(src);
    assert_eq!(
        rs,
        vec![
            Glyph::AptPackage { name: "aa".into() },
            Glyph::AptPackage { name: "aa".into() },
        ]
    );
}

#[test]
fn appending_string_to_list_is_a_type_error() {
    let src = r#"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = "a" ++ [ "b" ] } ] } ]
"#;
    assert_eq!(err_phase(src), emet::Phase::Type);
}

#[test]
fn appending_list_to_string_is_a_type_error() {
    let src = r#"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = [ "a" ] ++ "b" } ] } ]
"#;
    assert_eq!(err_phase(src), emet::Phase::Type);
}

#[test]
fn appending_ints_is_a_type_error() {
    let src = r#"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = String.fromInt (1 ++ 2) } ] } ]
"#;
    assert_eq!(err_phase(src), emet::Phase::Type);
}
