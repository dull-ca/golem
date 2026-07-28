//! Wave 1: type variables and type application in signatures.

mod common;

use common::{err_phase, single_scroll_glyphs};
use emet::Phase;

fn ok_main_len(src: &str) -> usize {
    single_scroll_glyphs(src).len()
}

#[test]
fn polymorphic_identity_with_signature_used_at_two_types() {
    let src = r#"
id : a -> a
id x = x
main = [ scroll { name = "test", glyphs = [ id (aptPackage { name = id "p" }) ] } ]
"#;
    assert_eq!(ok_main_len(src), 1);
}

#[test]
fn const_signature_with_two_type_variables_checks() {
    let src = r#"
const : a -> b -> a
const x y = x
main = [ scroll { name = "test", glyphs = [ const (aptPackage { name = "nginx" }) "ignored" ] } ]
"#;
    assert_eq!(ok_main_len(src), 1);
}

#[test]
fn type_application_in_signature_parses_and_checks() {
    let src = r#"
glyphs : List Glyph
glyphs = [ aptPackage { name = "nginx" } ]
main : List Scroll
main = [ scroll { name = "test", glyphs = glyphs } ]
"#;
    assert_eq!(ok_main_len(src), 1);
}

#[test]
fn signature_variable_forced_to_conflicting_concretes_is_a_type_error() {
    // `pkg` takes a String and returns an AptPackage, so `a -> a` forces the
    // one signature variable to unify with both String and AptPackage.
    let src = r#"
pkg : a -> a
pkg name = aptPackage { name = name }
main = [ scroll { name = "test", glyphs = [ pkg "nginx" ] } ]
"#;
    assert_eq!(err_phase(src), Phase::Type);
}

#[test]
fn unknown_type_head_in_signature_is_a_type_error() {
    let src = r#"
foo : Bogus a
foo = [ ]
main = [ scroll { name = "test", glyphs = [ ] } ]
"#;
    assert_eq!(err_phase(src), Phase::Type);
}

#[test]
fn overapplied_type_head_in_signature_is_a_type_error() {
    let src = r#"
foo : String a
foo = "x"
main = [ scroll { name = "test", glyphs = [ ] } ]
"#;
    assert_eq!(err_phase(src), Phase::Type);
}
