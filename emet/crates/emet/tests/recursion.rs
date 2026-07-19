//! General self-recursion: a top-level (or `let`) decl's name is in scope
//! within its own body, so users can write recursive functions. Numeric
//! results are observed by rendering them into a glyph `unit` string via
//! `String.fromInt`.

mod common;

use emet::{ir::Glyph, Phase};

fn unit(src: &str) -> String {
    match common::single_scroll_glyphs(src).as_slice() {
        [Glyph::SystemdService { unit }] => unit.clone(),
        other => panic!("expected a single systemdService glyph, got {other:?}"),
    }
}

#[test]
fn factorial_computes_by_self_recursion() {
    let src = r#"
fact n = if n == 0 then 1 else n * fact (n - 1)
main = [ scroll { name = "test", glyphs = [ systemdService { unit = String.fromInt (fact 5) } ] } ]
"#;
    assert_eq!(unit(src), "120");
}

#[test]
fn fibonacci_double_self_recursion() {
    let src = r#"
fib n = if n == 0 then 0 else if n == 1 then 1 else fib (n - 1) + fib (n - 2)
main = [ scroll { name = "test", glyphs = [ systemdService { unit = String.fromInt (fib 10) } ] } ]
"#;
    assert_eq!(unit(src), "55");
}

#[test]
fn recursion_builds_a_glyph_list() {
    // A recursive builder appends one glyph per step, producing n glyphs.
    let src = r#"
services n = if n == 0 then [] else List.append [ systemdService { unit = String.append (String.fromInt n) ".service" } ] (services (n - 1))
main = [ scroll { name = "test", glyphs = services 4 } ]
"#;
    let glyphs = common::single_scroll_glyphs(src);
    assert_eq!(glyphs.len(), 4);
    assert_eq!(glyphs[0], Glyph::SystemdService { unit: "4.service".into() });
    assert_eq!(glyphs[3], Glyph::SystemdService { unit: "1.service".into() });
}

#[test]
fn non_recursive_decl_still_generalizes_polymorphically() {
    // A non-recursive decl must still generalize (used at two types here).
    let src = r#"
id x = x
main = [ scroll { name = "test", glyphs = [ id (aptPackage { name = id "p" }) ] } ]
"#;
    assert_eq!(common::single_scroll_glyphs(src).len(), 1);
}

#[test]
fn self_recursion_via_let() {
    let src = r#"
main =
  let countdown n = if n == 0 then 0 else countdown (n - 1)
  in [ scroll { name = "test", glyphs = [ systemdService { unit = String.fromInt (countdown 7) } ] } ]
"#;
    assert_eq!(unit(src), "0");
}

#[test]
fn infinite_recursion_hits_recursion_limit() {
    // An obviously non-terminating recursion must surface a clean limit error
    // rather than overflowing the stack or hanging.
    let src = r#"
loop n = loop n
main = [ scroll { name = "test", glyphs = [ systemdService { unit = String.fromInt (loop 0) } ] } ]
"#;
    let e = common::err(src);
    assert_eq!(e.phase, Phase::Analyze);
    assert!(
        e.msg.contains("recursion limit"),
        "expected a recursion-limit error, got: {}",
        e.msg
    );
}
