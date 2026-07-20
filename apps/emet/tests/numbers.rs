//! Numeric literals, `Int`/`Float`, `number`/`comparable` bounded variables
//! with `Int` defaulting, and Elm-precedence infix operators. Numeric and
//! boolean results are observed by rendering them into the `unit` string of
//! a `systemdService` glyph via `String.fromInt`/`String.fromFloat`.

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

/// Render an `Int`-typed expression `e` into an observable glyph unit.
fn int_program(expr: &str) -> String {
    common::one_scroll(&format!("[ systemdService {{ unit = String.fromInt ({expr}) }} ]"))
}

/// Render a `Float`-typed expression `e` into an observable glyph unit.
fn float_program(expr: &str) -> String {
    common::one_scroll(&format!("[ systemdService {{ unit = String.fromFloat ({expr}) }} ]"))
}

/// Render a `Bool`-typed expression into `"yes"`/`"no"`.
fn bool_program(expr: &str) -> String {
    common::one_scroll(&format!(
        "[ systemdService {{ unit = if ({expr}) then \"yes\" else \"no\" }} ]"
    ))
}

#[test]
fn precedence_mul_binds_tighter_than_add() {
    assert_eq!(unit(&int_program("2 + 3 * 4")), "14");
}

#[test]
fn float_multiplication() {
    assert_eq!(unit(&float_program("2.0 * 3.0")), "6");
}

#[test]
fn integer_division_truncates() {
    assert_eq!(unit(&int_program("10 // 3")), "3");
}

#[test]
fn mod_by_wraps() {
    assert_eq!(unit(&int_program("modBy 3 10")), "1");
}

#[test]
fn power_is_right_associative() {
    // 2 ^ 3 ^ 2 = 2 ^ (3 ^ 2) = 2 ^ 9 = 512
    assert_eq!(unit(&int_program("2 ^ 3 ^ 2")), "512");
    assert_eq!(unit(&int_program("2 ^ 3")), "8");
}

#[test]
fn comparison_yields_bool_true() {
    assert_eq!(unit(&bool_program("1 < 2")), "yes");
}

#[test]
fn chained_comparison_is_a_parse_error() {
    let src = bool_program("1 < 2 < 3");
    assert_eq!(err(&src).0, Phase::Parse);
}

#[test]
fn boolean_and() {
    assert_eq!(unit(&bool_program("True && False")), "no");
}

#[test]
fn boolean_or() {
    assert_eq!(unit(&bool_program("False || True")), "yes");
}

#[test]
fn prefix_not_is_a_function() {
    assert_eq!(unit(&bool_program("not True")), "no");
}

#[test]
fn bare_integer_literal_defaults_to_int() {
    // No `String.fromInt` coercion hint: `n = 3` alone must default to `Int`.
    let src = r#"
n = 3
main = [ scroll { name = "test", glyphs = [ systemdService { unit = String.fromInt n } ] } ]
"#;
    assert_eq!(unit(src), "3");
}

#[test]
fn float_literal_is_float() {
    assert_eq!(unit(&float_program("3.0")), "3");
}

#[test]
fn polymorphic_numeric_literal_used_at_int() {
    // `three` is inferred `number`; using it under `String.fromInt` fixes `Int`.
    let src = r#"
three = 1 + 2
main = [ scroll { name = "test", glyphs = [ systemdService { unit = String.fromInt three } ] } ]
"#;
    assert_eq!(unit(src), "3");
}

#[test]
fn unary_minus_negates() {
    assert_eq!(unit(&int_program("-5")), "-5");
}

#[test]
fn negate_function() {
    assert_eq!(unit(&int_program("negate 5")), "-5");
}

#[test]
fn unary_minus_in_expression() {
    assert_eq!(unit(&int_program("10 + -3")), "7");
}

#[test]
fn number_to_string_in_glyph_unit() {
    let src = r#"
port = 8080
main = [ scroll { name = "test", glyphs = [ systemdService { unit = String.append (String.fromInt port) ".service" } ] } ]
"#;
    assert_eq!(unit(src), "8080.service");
}

#[test]
fn append_operator_on_strings() {
    let src = r#"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = "a" ++ "b" ++ "c" } ] } ]
"#;
    assert_eq!(unit(src), "abc");
}

#[test]
fn list_length_is_int() {
    assert_eq!(unit(&int_program("List.length [ \"a\", \"b\", \"c\" ]")), "3");
}

#[test]
fn list_range_builds_a_list() {
    assert_eq!(unit(&int_program("List.length (List.range 1 3)")), "3");
}

#[test]
fn list_sum_folds_numbers() {
    assert_eq!(unit(&int_program("List.sum (List.range 1 3)")), "6");
}

#[test]
fn compare_returns_order_usable_in_case() {
    let src = r#"
picked =
  case compare 1 2 of
    LT -> "less"
    EQ -> "equal"
    GT -> "greater"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = picked } ] } ]
"#;
    assert_eq!(unit(src), "less");
}

#[test]
fn string_plus_number_is_a_type_error() {
    assert_eq!(err(&int_program("\"a\" + 1")).0, Phase::Type);
}

#[test]
fn comparing_across_types_is_a_type_error() {
    let src = bool_program("1 == \"a\"");
    assert_eq!(err(&src).0, Phase::Type);
}

#[test]
fn float_division_by_zero_is_total() {
    assert_eq!(unit(&float_program("1.0 / 0.0")), "0");
}

#[test]
fn integer_division_by_zero_is_total() {
    assert_eq!(unit(&int_program("1 // 0")), "0");
}

#[test]
fn mod_by_zero_is_total() {
    assert_eq!(unit(&int_program("modBy 0 5")), "0");
}
