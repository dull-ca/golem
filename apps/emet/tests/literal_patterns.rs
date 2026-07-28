//! Elm-parity coverage for literal patterns (ADR 0026): int, char, and string
//! evaluation; exhaustiveness needs a trailing `_`; duplicate/after-catch-all
//! arms are redundant; float patterns are a parse error carrying the redirect
//! message; a negative-int pattern (`-1`); and the check that an int pattern
//! types the scrutinee as `number`, not `Int`, so it matches a `Float` value.

mod common;

use emet::{ir::Glyph, Phase};

fn unit(src: &str) -> String {
    match common::single_scroll_glyphs(src).as_slice() {
        [Glyph::SystemdService { unit }] => unit.clone(),
        other => panic!("expected a single systemdService glyph, got {other:?}"),
    }
}

fn err(src: &str) -> (Phase, String) {
    let e = common::err(src);
    (e.phase, e.msg)
}

#[test]
fn int_case_selects_matching_arm() {
    let src = r#"
label n =
  case n of
    0 -> "zero"
    1 -> "one"
    _ -> "many"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = label 1 } ] } ]
"#;
    assert_eq!(unit(src), "one");
}

#[test]
fn int_case_falls_through_to_catch_all() {
    let src = r#"
label n =
  case n of
    0 -> "zero"
    _ -> "other"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = label 7 } ] } ]
"#;
    assert_eq!(unit(src), "other");
}

#[test]
fn char_case_selects_matching_arm() {
    let src = r#"
label c =
  case c of
    'a' -> "alpha"
    'b' -> "bravo"
    _ -> "other"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = label 'b' } ] } ]
"#;
    assert_eq!(unit(src), "bravo");
}

#[test]
fn string_case_still_works() {
    let src = r#"
label s =
  case s of
    "hi" -> "greeting"
    _ -> "other"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = label "hi" } ] } ]
"#;
    assert_eq!(unit(src), "greeting");
}

#[test]
fn negative_int_pattern_matches() {
    let src = r#"
label n =
  case n of
    -1 -> "neg-one"
    0 -> "zero"
    _ -> "other"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = label (-1) } ] } ]
"#;
    assert_eq!(unit(src), "neg-one");
}

#[test]
fn int_case_without_catch_all_is_non_exhaustive() {
    let src = r#"
label n =
  case n of
    0 -> "zero"
    1 -> "one"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = label 0 } ] } ]
"#;
    assert_eq!(err(src).0, Phase::Type);
}

#[test]
fn int_case_with_catch_all_is_exhaustive() {
    let src = r#"
label n =
  case n of
    0 -> "zero"
    _ -> "other"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = label 0 } ] } ]
"#;
    assert_eq!(unit(src), "zero");
}

#[test]
fn char_case_without_catch_all_is_non_exhaustive() {
    let src = r#"
label c =
  case c of
    'a' -> "alpha"
    'b' -> "bravo"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = label 'a' } ] } ]
"#;
    assert_eq!(err(src).0, Phase::Type);
}

#[test]
fn duplicate_int_literal_arm_is_redundant() {
    let src = r#"
label n =
  case n of
    0 -> "a"
    0 -> "b"
    _ -> "c"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = label 0 } ] } ]
"#;
    assert_eq!(err(src).0, Phase::Type);
}

#[test]
fn int_arm_after_catch_all_is_redundant() {
    let src = r#"
label n =
  case n of
    _ -> "a"
    0 -> "b"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = label 0 } ] } ]
"#;
    assert_eq!(err(src).0, Phase::Type);
}

#[test]
fn float_literal_pattern_is_rejected_with_comparison_hint() {
    let src = r#"
label x =
  case x of
    3.14 -> "pi"
    _ -> "other"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = label 1.0 } ] } ]
"#;
    let (phase, msg) = err(src);
    assert_eq!(phase, Phase::Parse);
    let lowered = msg.to_lowercase();
    assert!(
        lowered.contains("float"),
        "message should mention floats: {msg}"
    );
    assert!(
        lowered.contains("equality") || lowered.contains("compare") || lowered.contains("<"),
        "message should steer to comparison operators: {msg}"
    );
}

#[test]
fn negative_float_literal_pattern_is_rejected() {
    let src = r#"
label x =
  case x of
    -3.14 -> "neg-pi"
    _ -> "other"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = label 1.0 } ] } ]
"#;
    let (phase, msg) = err(src);
    assert_eq!(phase, Phase::Parse);
    assert!(
        msg.to_lowercase().contains("float"),
        "message should mention floats: {msg}"
    );
}

#[test]
fn int_literal_pattern_leaves_scrutinee_polymorphic_number() {
    let src = r#"
render x =
  case x of
    0 -> "zero"
    _ -> String.fromFloat x
main = [ scroll { name = "test", glyphs = [ systemdService { unit = render 2.5 } ] } ]
"#;
    assert_eq!(unit(src), "2.5");
}
