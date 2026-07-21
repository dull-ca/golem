mod common;

use common::{err_phase, single_scroll_glyphs};
use emet::Phase;

fn compiles(src: &str) {
    let _ = single_scroll_glyphs(src);
}

#[test]
fn over_general_identity_on_monomorphic_body_is_rejected() {
    let src = r#"
f : a -> a
f x = x + 1
main = [ scroll { name = "test", glyphs = [ ] } ]
"#;
    assert_eq!(err_phase(src), Phase::Type);
}

#[test]
fn signature_keeping_two_vars_distinct_that_body_forces_equal_is_rejected() {
    let src = r#"
pair : a -> b -> List a
pair x y = [ x, y ]
main = [ scroll { name = "test", glyphs = [ ] } ]
"#;
    assert_eq!(err_phase(src), Phase::Type);
}

#[test]
fn signature_variable_forced_to_concrete_return_is_rejected() {
    let src = r#"
firstChar : a -> String
firstChar x = x
main = [ scroll { name = "test", glyphs = [ ] } ]
"#;
    assert_eq!(err_phase(src), Phase::Type);
}

#[test]
fn identity_signature_is_accepted() {
    let src = r#"
id : a -> a
id x = x
main = [ scroll { name = "test", glyphs = [ id (aptPackage { name = "nginx" }) ] } ]
"#;
    compiles(src);
}

#[test]
fn const_signature_is_accepted() {
    let src = r#"
const : a -> b -> a
const x y = x
main = [ scroll { name = "test", glyphs = [ const (aptPackage { name = "nginx" }) "ignored" ] } ]
"#;
    compiles(src);
}

#[test]
fn map_signature_is_accepted() {
    let src = r#"
apply : (a -> b) -> a -> b
apply f x = f x
myMap : (a -> b) -> List a -> List b
myMap f xs =
  case xs of
    [] -> []
    (h :: t) -> apply f h :: myMap f t
main = [ scroll { name = "test", glyphs = [ aptPackage { name = "nginx" } ] } ]
"#;
    compiles(src);
}

#[test]
fn polymorphic_recursive_signature_is_accepted() {
    let src = r#"
length : List a -> Int
length xs =
  case xs of
    [] -> 0
    (h :: t) -> 1 + length t
main = [ scroll { name = "test", glyphs = [ aptPackage { name = "nginx" } ] } ]
"#;
    compiles(src);
}

#[test]
fn correctly_monomorphic_signature_is_accepted() {
    let src = r#"
inc : Int -> Int
inc x = x + 1
main = [ scroll { name = "test", glyphs = [ aptPackage { name = "nginx" } ] } ]
"#;
    compiles(src);
}
