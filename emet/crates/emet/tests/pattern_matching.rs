//! `case … of`, `if … then … else`, and compile-time exhaustiveness /
//! redundancy checking. Values are observed through the `unit` string of a
//! `systemdService` glyph.

mod common;

use emet::{ir::Glyph, Phase};

/// Compile a program whose single scroll holds one `systemdService` glyph and
/// return its `unit` string.
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
fn case_selects_just_arm() {
    let src = r#"
picked =
  case Just "x" of
    Just y -> y
    Nothing -> "d"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = picked } ] } ]
"#;
    assert_eq!(unit(src), "x");
}

#[test]
fn case_selects_nothing_arm() {
    let src = r#"
scrut : Maybe String
scrut = Nothing
picked =
  case scrut of
    Just y -> y
    Nothing -> "d"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = picked } ] } ]
"#;
    assert_eq!(unit(src), "d");
}

#[test]
fn if_true_takes_then_branch() {
    let src = r#"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = if True then "t" else "e" } ] } ]
"#;
    assert_eq!(unit(src), "t");
}

#[test]
fn if_false_takes_else_branch() {
    let src = r#"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = if False then "t" else "e" } ] } ]
"#;
    assert_eq!(unit(src), "e");
}

#[test]
fn if_condition_must_be_bool() {
    let src = r#"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = if "nope" then "t" else "e" } ] } ]
"#;
    assert_eq!(err(src).0, Phase::Type);
}

#[test]
fn non_exhaustive_case_names_missing_constructor() {
    let src = r#"
scrut : Maybe String
scrut = Nothing
picked =
  case scrut of
    Just y -> y
main = [ scroll { name = "test", glyphs = [ systemdService { unit = picked } ] } ]
"#;
    let (phase, msg) = err(src);
    assert_eq!(phase, Phase::Type);
    assert!(msg.contains("Nothing"), "message should name the missing constructor: {msg}");
}

#[test]
fn redundant_duplicate_constructor_arm_errors() {
    let src = r#"
scrut : Maybe String
scrut = Nothing
picked =
  case scrut of
    Just y -> y
    Nothing -> "a"
    Nothing -> "b"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = picked } ] } ]
"#;
    assert_eq!(err(src).0, Phase::Type);
}

#[test]
fn redundant_arm_after_catch_all_errors() {
    let src = r#"
scrut : Maybe String
scrut = Nothing
picked =
  case scrut of
    other -> "a"
    Nothing -> "b"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = picked } ] } ]
"#;
    assert_eq!(err(src).0, Phase::Type);
}

#[test]
fn string_case_without_catch_all_errors() {
    let src = r#"
picked =
  case "hi" of
    "hi" -> "a"
    "bye" -> "b"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = picked } ] } ]
"#;
    assert_eq!(err(src).0, Phase::Type);
}

#[test]
fn string_case_with_catch_all_ok() {
    let src = r#"
picked =
  case "hi" of
    "hi" -> "matched"
    other -> other
main = [ scroll { name = "test", glyphs = [ systemdService { unit = picked } ] } ]
"#;
    assert_eq!(unit(src), "matched");
}

#[test]
fn glyph_scrutinee_constructor_pattern_is_unknown_constructor() {
    let src = r#"
picked =
  case aptPackage { name = "nginx" } of
    AptPackage p -> "matched"
    other -> "d"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = picked } ] } ]
"#;
    let (phase, msg) = err(src);
    assert_eq!(phase, Phase::Type);
    assert!(msg.contains("AptPackage"), "message should name the unknown constructor: {msg}");
}

#[test]
fn glyph_scrutinee_wildcard_is_ok() {
    let src = r#"
picked =
  case aptPackage { name = "nginx" } of
    other -> "wild"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = picked } ] } ]
"#;
    assert_eq!(unit(src), "wild");
}

#[test]
fn nested_pattern_checks_and_evaluates() {
    let src = r#"
scrut : Maybe (Maybe String)
scrut = Just (Just "deep")
picked =
  case scrut of
    Just (Just x) -> x
    Just Nothing -> "inner-nothing"
    Nothing -> "outer-nothing"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = picked } ] } ]
"#;
    assert_eq!(unit(src), "deep");
}

#[test]
fn nested_pattern_non_exhaustive_errors() {
    let src = r#"
scrut : Maybe (Maybe String)
scrut = Just (Just "deep")
picked =
  case scrut of
    Just (Just x) -> x
    Nothing -> "outer-nothing"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = picked } ] } ]
"#;
    assert_eq!(err(src).0, Phase::Type);
}

#[test]
fn wildcard_covers_remaining_constructors() {
    let src = r#"
scrut : Maybe String
scrut = Just "y"
picked =
  case scrut of
    Just x -> x
    _ -> "d"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = picked } ] } ]
"#;
    assert_eq!(unit(src), "y");
}

#[test]
fn arm_bodies_must_share_a_type() {
    let src = r#"
scrut : Maybe String
scrut = Nothing
picked =
  case scrut of
    Just y -> y
    Nothing -> True
main = [ scroll { name = "test", glyphs = [ systemdService { unit = picked } ] } ]
"#;
    assert_eq!(err(src).0, Phase::Type);
}
