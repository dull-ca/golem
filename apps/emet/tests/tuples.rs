//! The Elm-parity oracle for tuples and unit (ADR 0027). Each test pins one
//! behavior against how Elm behaves: building and destructuring 2- and 3-tuples,
//! unit `()`, nested tuple patterns, a tuple `case` that is exhaustive with no
//! catch-all (and its non-exhaustive counterpart, which must be flagged),
//! lexicographic comparison, rejection of a tuple over non-comparable elements,
//! the 4-tuple parse-time redirect to a record, `String.uncons` on empty and
//! non-empty input, and each `Tuple` module function.

mod common;

use emet::{ir::Glyph, Phase};

fn unit(src: &str) -> String {
    match common::single_scroll_glyphs(src).as_slice() {
        [Glyph::SystemdService { unit }] => unit.clone(),
        other => panic!("expected a single systemdService glyph, got {other:?}"),
    }
}

fn string_of(expr: &str) -> String {
    unit(&common::one_scroll(&format!(
        "[ systemdService {{ unit = {expr} }} ]"
    )))
}

fn int_of(expr: &str) -> String {
    string_of(&format!("String.fromInt ({expr})"))
}

fn bool_of(expr: &str) -> String {
    string_of(&format!("if ({expr}) then \"yes\" else \"no\""))
}

fn err(src: &str) -> (Phase, String) {
    let e = common::err(src);
    (e.phase, e.msg)
}

#[test]
fn pair_builds_and_destructures() {
    let src = r#"
main : List Scroll
main =
  let
    p = (1, 2)
  in
  case p of
    (a, b) -> [ scroll { name = "t", glyphs = [ systemdService { unit = String.fromInt (a + b) } ] } ]
"#;
    assert_eq!(unit(src), "3");
}

#[test]
fn triple_builds_and_destructures() {
    let src = r#"
main : List Scroll
main =
  case ("a", "b", "c") of
    (x, y, z) -> [ scroll { name = "t", glyphs = [ systemdService { unit = String.concat [x, y, z] } ] } ]
"#;
    assert_eq!(unit(src), "abc");
}

#[test]
fn unit_value_builds_and_matches() {
    let src = r#"
main : List Scroll
main =
  case () of
    () -> [ scroll { name = "t", glyphs = [ systemdService { unit = "ok" } ] } ]
"#;
    assert_eq!(unit(src), "ok");
}

#[test]
fn nested_tuple_pattern_in_case() {
    let src = r#"
main : List Scroll
main =
  case (Just 1, (2, 3)) of
    (Just a, (b, c)) -> [ scroll { name = "t", glyphs = [ systemdService { unit = String.fromInt (a + b + c) } ] } ]
    (Nothing, (b, c)) -> [ scroll { name = "t", glyphs = [ systemdService { unit = String.fromInt (b + c) } ] } ]
"#;
    assert_eq!(unit(src), "6");
}

#[test]
fn tuple_case_exhaustive_without_catch_all() {
    let src = r#"
main : List Scroll
main =
  case (True, False) of
    (True, b) -> [ scroll { name = "t", glyphs = [ systemdService { unit = "a" } ] } ]
    (False, True) -> [ scroll { name = "t", glyphs = [ systemdService { unit = "b" } ] } ]
    (False, False) -> [ scroll { name = "t", glyphs = [ systemdService { unit = "c" } ] } ]
"#;
    assert_eq!(unit(src), "a");
}

#[test]
fn tuple_case_non_exhaustive_is_flagged() {
    let src = r#"
main : List Scroll
main =
  case (True, False) of
    (True, b) -> [ scroll { name = "t", glyphs = [ systemdService { unit = "a" } ] } ]
    (False, True) -> [ scroll { name = "t", glyphs = [ systemdService { unit = "b" } ] } ]
"#;
    assert_eq!(err(src).0, Phase::Type);
}

#[test]
fn tuple_comparison_is_lexicographic() {
    assert_eq!(bool_of("(1, 2) < (1, 3)"), "yes");
    assert_eq!(bool_of("(1, 3) < (1, 2)"), "no");
    assert_eq!(bool_of("(2, 0) < (1, 9)"), "no");
    assert_eq!(bool_of("(1, 2) == (1, 2)"), "yes");
}

#[test]
fn tuple_of_non_comparables_is_type_error() {
    let src = &common::one_scroll(
        r#"[ systemdService { unit = if ((\x -> x, 1) < (\y -> y, 2)) then "a" else "b" } ]"#,
    );
    assert_eq!(err(src).0, Phase::Type);
}

#[test]
fn four_tuple_is_rejected_with_record_redirect() {
    let src = r#"
main : List Scroll
main = tuple

tuple = (1, 2, 3, 4)
"#;
    let (phase, msg) = err(src);
    assert_eq!(phase, Phase::Parse);
    assert!(
        msg.contains('3'),
        "message should mention the 3-element cap: {msg}"
    );
    let lowered = msg.to_lowercase();
    assert!(
        lowered.contains("record"),
        "message should steer to a record: {msg}"
    );
}

#[test]
fn string_uncons_empty_is_nothing() {
    let src = r#"
main : List Scroll
main =
  case String.uncons "" of
    Nothing -> [ scroll { name = "t", glyphs = [ systemdService { unit = "empty" } ] } ]
    Just _ -> [ scroll { name = "t", glyphs = [ systemdService { unit = "some" } ] } ]
"#;
    assert_eq!(unit(src), "empty");
}

#[test]
fn string_uncons_nonempty_destructured_via_tuple_pattern() {
    let src = r#"
main : List Scroll
main =
  case String.uncons "ab" of
    Just (c, rest) -> [ scroll { name = "t", glyphs = [ systemdService { unit = String.concat [ String.fromChar c, rest ] } ] } ]
    Nothing -> [ scroll { name = "t", glyphs = [ systemdService { unit = "empty" } ] } ]
"#;
    assert_eq!(unit(src), "ab");
}

#[test]
fn tuple_pair_first_second() {
    assert_eq!(int_of("Tuple.first (Tuple.pair 7 9)"), "7");
    assert_eq!(int_of("Tuple.second (Tuple.pair 7 9)"), "9");
}

#[test]
fn tuple_map_first() {
    assert_eq!(
        int_of("Tuple.first (Tuple.mapFirst (\\x -> x + 1) (10, 20))"),
        "11"
    );
    assert_eq!(
        int_of("Tuple.second (Tuple.mapFirst (\\x -> x + 1) (10, 20))"),
        "20"
    );
}

#[test]
fn tuple_map_second() {
    assert_eq!(
        int_of("Tuple.second (Tuple.mapSecond (\\x -> x + 1) (10, 20))"),
        "21"
    );
    assert_eq!(
        int_of("Tuple.first (Tuple.mapSecond (\\x -> x + 1) (10, 20))"),
        "10"
    );
}

#[test]
fn tuple_map_both() {
    let mapped = "Tuple.mapBoth String.toUpper (\\n -> n + 100) (\"hi\", 5)";
    assert_eq!(string_of(&format!("Tuple.first ({mapped})")), "HI");
    assert_eq!(int_of(&format!("Tuple.second ({mapped})")), "105");
}
