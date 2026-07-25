//! Parser/type diagnostics spec: pins the structured `emet::Error` (phase,
//! a message substring, and a span) that `compile()` returns for each class
//! of bad input, since that's what the `ariadne` renderer in `main.rs`
//! ultimately displays.

use emet::{compile, Error, Phase};

fn err(src: &str) -> Error {
    match compile(src) {
        Ok(_) => panic!("expected a compile error, but compilation succeeded"),
        Err(e) => e,
    }
}

fn assert_anchored_away_from_module_start(e: &Error) {
    assert_ne!(e.span, 0..0, "span must not be the empty sentinel: {e:?}");
    assert_ne!(
        e.span,
        0..4,
        "span must not be the whole-module anchor `main`/first-token: {e:?}"
    );
}

#[test]
fn unclosed_bracket_reports_closing_bracket() {
    let e = err(r#"main = [ "a" "#);
    assert_eq!(e.phase, Phase::Parse);
    assert!(e.msg.contains("']'"), "msg: {}", e.msg);
    assert_eq!(e.span, 13..13);
}

#[test]
fn leading_junk_reports_the_offending_token() {
    let e = err(r#"main = )"#);
    assert_eq!(e.phase, Phase::Parse);
    assert!(e.msg.contains("')'"), "msg: {}", e.msg);
    assert!(e.msg.contains("expected"), "msg: {}", e.msg);
    assert_eq!(e.span, 7..8);
}

#[test]
fn apt_package_missing_name_field() {
    let e = err(r#"main = [ aptPackage { unit = "u" } ]"#);
    assert_eq!(e.phase, Phase::Parse);
    assert!(
        e.msg.contains("`aptPackage` requires a `name` field"),
        "msg: {}",
        e.msg
    );
    assert_eq!(e.span, 9..19);
}

#[test]
fn systemd_service_missing_unit_field() {
    let e = err(r#"main = [ systemdService { name = "n" } ]"#);
    assert_eq!(e.phase, Phase::Parse);
    assert!(
        e.msg.contains("`systemdService` requires a `unit` field"),
        "msg: {}",
        e.msg
    );
    assert_eq!(e.span, 9..23);
}

#[test]
fn apt_package_unknown_field() {
    let e = err(r#"main = [ aptPackage { name = "n", foo = "x" } ]"#);
    assert_eq!(e.phase, Phase::Parse);
    assert!(
        e.msg.contains("unknown aptPackage field `foo`"),
        "msg: {}",
        e.msg
    );
    assert_eq!(e.span, 9..19);
}

#[test]
fn systemd_service_unknown_field() {
    let e = err(r#"main = [ systemdService { unit = "u", foo = "x" } ]"#);
    assert_eq!(e.phase, Phase::Parse);
    assert!(
        e.msg.contains("unknown systemdService field `foo`"),
        "msg: {}",
        e.msg
    );
    assert_eq!(e.span, 9..23);
}

// Wave 2 made `[t]` general list-type sugar, so `[String]` is a valid
// `List String` signature rather than the old `[Glyph]`-only parse error.
#[test]
fn list_type_sugar_in_signature_checks() {
    let src = "names : [String]\nnames = [ \"a\", \"b\" ]\nmain = [ ]";
    let c = emet::compile(src).expect("`[String]` should parse and check");
    assert_eq!(c.main_ty.to_string(), "List Scroll");
}

#[test]
fn bad_type_after_colon() {
    let e = err(r#"foo : )"#);
    assert_eq!(e.phase, Phase::Parse);
    assert!(e.msg.contains("a type"), "msg: {}", e.msg);
    assert_eq!(e.span, 6..7);
}

#[test]
fn let_without_in_is_a_parse_error() {
    let e = err(r#"main = let x = "a""#);
    assert_eq!(e.phase, Phase::Parse);
    assert!(e.msg.contains("expected"), "msg: {}", e.msg);
    assert_anchored_away_from_module_start(&e);
}

#[test]
fn lambda_without_arrow_is_a_parse_error() {
    let e = err(r#"main = \x"#);
    assert_eq!(e.phase, Phase::Parse);
    assert_eq!(e.span, 7..8);
}

#[test]
fn lambda_without_params_is_a_parse_error() {
    let e = err(r#"main = \ -> x"#);
    assert_eq!(e.phase, Phase::Parse);
    assert_eq!(e.span, 7..8);
}

#[test]
fn field_access_without_name() {
    let e = err(r#"main = x."#);
    assert_eq!(e.phase, Phase::Parse);
    assert_anchored_away_from_module_start(&e);
    assert_eq!(e.span, 8..9);
}

#[test]
fn signature_name_mismatch() {
    let e = err("f : String\ng = f");
    assert_eq!(e.phase, Phase::Parse);
    assert!(e.msg.contains("signature names"), "msg: {}", e.msg);
    assert!(e.msg.contains("`f`"), "msg: {}", e.msg);
    assert!(e.msg.contains("`g`"), "msg: {}", e.msg);
}

#[test]
fn signature_with_no_binding() {
    let e = err(r#"f : String"#);
    assert_eq!(e.phase, Phase::Parse);
    assert!(e.msg.contains("no accompanying binding"), "msg: {}", e.msg);
}

#[test]
fn trailing_signature_with_no_binding() {
    let e = err("f : String\nf = \"a\"\ng : String");
    assert_eq!(e.phase, Phase::Parse);
    assert!(e.msg.contains("no accompanying binding"), "msg: {}", e.msg);
    assert!(e.msg.contains("`g`"), "msg: {}", e.msg);
}

#[test]
fn two_signatures_for_same_name() {
    let e = err("f : String\nf : String\nf = \"a\"");
    assert_eq!(e.phase, Phase::Parse);
    assert!(e.msg.contains("unused signature"), "msg: {}", e.msg);
}

#[test]
fn unknown_name_is_type_error_with_message() {
    let e = err(r#"main = [ nope ]"#);
    assert_eq!(e.phase, Phase::Type);
    assert!(e.msg.contains("unknown name"), "msg: {}", e.msg);
    assert!(e.msg.contains("`nope`"), "msg: {}", e.msg);
    assert_eq!(e.span, 9..13);
}

#[test]
fn wrong_field_type_is_type_error() {
    let e = err(r#"main = [ systemdService { unit = aptPackage { name = "x" } } ]"#);
    assert_eq!(e.phase, Phase::Type);
    assert!(e.msg.contains("type mismatch"), "msg: {}", e.msg);
}

#[test]
fn signature_conflict_is_type_error() {
    let e = err(
        "webserver : SystemdService\nwebserver unit = systemdService { unit = unit }\nmain = [ webserver \"x\" ]",
    );
    assert_eq!(e.phase, Phase::Type);
    assert!(e.msg.contains("type mismatch"), "msg: {}", e.msg);
}

#[test]
fn missing_main_is_type_error_with_message() {
    let e = err(r#"foo = "bar""#);
    assert_eq!(e.phase, Phase::Type);
    assert!(e.msg.contains("main"), "msg: {}", e.msg);
}

#[test]
fn str_alias_is_unknown_type_constructor() {
    let e = err("f : Str -> Str\nf x = x\nmain = [ scroll { name = \"n\", glyphs = [] } ]");
    assert_eq!(e.phase, Phase::Type);
    assert!(e.msg.contains("unknown type constructor"), "msg: {}", e.msg);
    assert!(e.msg.contains("`Str`"), "msg: {}", e.msg);
}

#[test]
fn glyphs_alias_is_unknown_type_constructor() {
    let e = err("f : Glyphs\nf = []\nmain = [ scroll { name = \"n\", glyphs = [] } ]");
    assert_eq!(e.phase, Phase::Type);
    assert!(e.msg.contains("unknown type constructor"), "msg: {}", e.msg);
    assert!(e.msg.contains("`Glyphs`"), "msg: {}", e.msg);
}

#[test]
fn arity_too_many_does_not_leak_internal_typevars() {
    let e = err("f : Int -> Int\nf x = x\nmain = [ f 1 2 ]");
    assert_eq!(e.phase, Phase::Type);
    assert!(!e.msg.contains("t1"), "leaked internal typevar: {}", e.msg);
    assert!(!e.msg.contains("t2"), "leaked internal typevar: {}", e.msg);
    assert!(!e.msg.contains("t9"), "leaked internal typevar: {}", e.msg);
}

#[test]
fn record_mismatch_renders_both_sides_through_one_shared_letter_map() {
    let e = err("f : { x : a } -> Int\nf r = 1\n\ng h = f { x = h, y = h }\n\nmain = [ g ]");
    assert_eq!(e.phase, Phase::Type);
    assert!(e.msg.contains("record types differ"), "msg: {}", e.msg);
    assert!(
        e.msg.contains("`{ x : a }`") && e.msg.contains("`{ x : a, y : a }`"),
        "internal type var must render through the friendly letter map, got: {}",
        e.msg
    );
    assert!(
        !e.msg.contains("t5") && !e.msg.contains("t6"),
        "leaked internal typevar: {}",
        e.msg
    );
}

#[test]
fn mismatch_renders_both_sides_through_one_shared_letter_map() {
    let e = err("f : (a, b) -> Int\nf t = 1\n\ng : (a -> b) -> Int\ng h = f h\n\nmain = [ g ]");
    assert_eq!(e.phase, Phase::Type);
    assert!(e.msg.contains("type mismatch"), "msg: {}", e.msg);
    assert!(
        e.msg.contains("expected `(a, b)`") && e.msg.contains("found `c -> d`"),
        "distinct internal vars must render to distinct letters across the two sides, got: {}",
        e.msg
    );
}

#[test]
fn occurs_error_renders_friendly_typevars() {
    let e = err("main = (\\x -> x x)");
    assert_eq!(e.phase, Phase::Type);
    assert!(!e.msg.contains("t1"), "leaked internal typevar: {}", e.msg);
}

#[test]
fn expected_set_has_no_virtual_semicolon() {
    let e = err("main =\n  let x = 1\n  x");
    assert_eq!(e.phase, Phase::Parse);
    assert!(!e.msg.contains("';'"), "virtual ; leaked: {}", e.msg);
}

#[test]
fn expected_set_has_no_something_else() {
    let e = err("f x x + 1\n\nmain = f 2");
    assert_eq!(e.phase, Phase::Parse);
    assert!(!e.msg.contains("something else"), "jargon leaked: {}", e.msg);
    assert!(e.msg.contains("an expression"), "expected replacement: {}", e.msg);
}

#[test]
fn expected_set_dedupes_repeated_brace() {
    let e = err("main = x + * y");
    assert_eq!(e.phase, Phase::Parse);
    let n = e.msg.matches("'}'").count();
    assert!(n <= 1, "duplicate '}}' in: {}", e.msg);
}

#[test]
fn unclosed_bracket_message_mentions_unclosed() {
    let e = err(r#"main = [ "a" "#);
    assert_eq!(e.phase, Phase::Parse);
    assert!(
        e.msg.to_lowercase().contains("close") || e.msg.to_lowercase().contains("unclosed"),
        "expected an unclosed hint: {}",
        e.msg
    );
    assert!(e.msg.contains("']'"), "should still name the delimiter: {}", e.msg);
}

#[test]
fn binding_a_reserved_word_is_rejected() {
    let e = err("keep n = n + 1\n\nmain = [ ]");
    assert_eq!(e.phase, Phase::Parse);
    assert!(e.msg.contains("`keep`"), "should name the word: {}", e.msg);
    assert!(e.msg.contains("reserved"), "should say reserved: {}", e.msg);
}

#[test]
fn braced_rollback_points_at_braceless_form() {
    let e = err(r#"main = [ scroll { name = "w", glyphs = [], policy = rollback { } } ]"#);
    assert_eq!(e.phase, Phase::Parse);
    assert!(e.msg.contains("without braces"), "msg: {}", e.msg);
}

#[test]
fn arrow_typo_hint() {
    let e = err("f x =\n  case x of\n    1 => \"a\"\n    _ => \"b\"\n\nmain = f 1");
    assert_eq!(e.phase, Phase::Parse);
    assert!(e.msg.contains("=>") && e.msg.contains("->"), "msg: {}", e.msg);
}

#[test]
fn missing_equals_hint() {
    let e = err("f x x + 1\n\nmain = f 2");
    assert_eq!(e.phase, Phase::Parse);
    assert!(e.msg.contains("="), "should mention '=': {}", e.msg);
    assert!(e.msg.to_lowercase().contains("definition") || e.msg.contains("'='"), "msg: {}", e.msg);
}

#[test]
fn empty_case_is_reported_as_no_arms() {
    let e = err("f x =\n  case x of\n\nmain = f 1");
    assert_eq!(e.phase, Phase::Type);
    assert!(e.msg.contains("no arms") || e.msg.contains("at least one"), "msg: {}", e.msg);
}

#[test]
fn number_constraint_is_plain_language() {
    let e = err("main = [ ]\nx = 1 + \"two\"");
    assert_eq!(e.phase, Phase::Type);
    assert!(!e.msg.contains("satisfy"), "jargon leaked: {}", e.msg);
    assert!(e.msg.to_lowercase().contains("number"), "msg: {}", e.msg);
    assert!(e.msg.contains("String"), "should name the offending type: {}", e.msg);
}

#[test]
fn if_condition_must_be_bool() {
    let e = err("main = [ ]\ny = if 1 then 2 else 3");
    assert_eq!(e.phase, Phase::Type);
    assert!(e.msg.contains("Bool"), "should mention Bool: {}", e.msg);
    assert!(e.msg.to_lowercase().contains("condition"), "msg: {}", e.msg);
}

#[test]
fn policy_field_wants_a_policy() {
    let e = err(r#"main = [ scroll { name = "w", glyphs = [], policy = "aggressive" } ]"#);
    assert_eq!(e.phase, Phase::Type);
    assert!(e.msg.contains("Policy"), "should mention Policy: {}", e.msg);
    assert!(!e.msg.contains("expected `String`"), "reversed framing leaked: {}", e.msg);
}
