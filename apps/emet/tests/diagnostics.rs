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
    assert!(e.msg.contains("`aptPackage` requires a `name` field"), "msg: {}", e.msg);
    assert_eq!(e.span, 9..19);
}

#[test]
fn systemd_service_missing_unit_field() {
    let e = err(r#"main = [ systemdService { name = "n" } ]"#);
    assert_eq!(e.phase, Phase::Parse);
    assert!(e.msg.contains("`systemdService` requires a `unit` field"), "msg: {}", e.msg);
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
    assert!(
        e.msg.contains("no accompanying binding"),
        "msg: {}",
        e.msg
    );
}

#[test]
fn trailing_signature_with_no_binding() {
    let e = err("f : String\nf = \"a\"\ng : String");
    assert_eq!(e.phase, Phase::Parse);
    assert!(
        e.msg.contains("no accompanying binding"),
        "msg: {}",
        e.msg
    );
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
    assert!(
        e.msg.contains("unknown type constructor"),
        "msg: {}",
        e.msg
    );
    assert!(e.msg.contains("`Str`"), "msg: {}", e.msg);
}

#[test]
fn glyphs_alias_is_unknown_type_constructor() {
    let e = err("f : Glyphs\nf = []\nmain = [ scroll { name = \"n\", glyphs = [] } ]");
    assert_eq!(e.phase, Phase::Type);
    assert!(
        e.msg.contains("unknown type constructor"),
        "msg: {}",
        e.msg
    );
    assert!(e.msg.contains("`Glyphs`"), "msg: {}", e.msg);
}
