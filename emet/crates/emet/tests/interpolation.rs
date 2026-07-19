//! String interpolation (`"a${e}b"`): end-to-end desugaring to `String.concat`,
//! brace-depth matching, nested strings, the `\${` escape, and the type errors
//! that fall out of the desugaring.

mod common;

use common::{err_phase, glyphs, single_scroll_glyphs};
use emet::ir::Glyph;

#[test]
fn interpolates_a_string_variable_into_a_unit() {
    let src = r#"
name = "nginx"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = "${name}.service" } ] } ]
"#;
    let rs = single_scroll_glyphs(src);
    assert_eq!(rs, vec![Glyph::SystemdService { unit: "nginx.service".into() }]);
}

#[test]
fn interpolates_a_number_via_from_int() {
    let src = r#"
port = 8080
main = [ scroll { name = "test", glyphs = [ systemdService { unit = "port ${String.fromInt port}" } ] } ]
"#;
    let rs = single_scroll_glyphs(src);
    assert_eq!(rs, vec![Glyph::SystemdService { unit: "port 8080".into() }]);
}

#[test]
fn multiple_and_adjacent_interpolations() {
    let src = r#"
a = "x"
b = "y"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = "${a}-${b} ${a}" } ] } ]
"#;
    let rs = single_scroll_glyphs(src);
    assert_eq!(rs, vec![Glyph::SystemdService { unit: "x-y x".into() }]);
}

#[test]
fn leading_and_trailing_interpolation_with_empty_chunk() {
    let src = r#"
a = "one"
b = "two"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = "${a}${b}" } ] } ]
"#;
    let rs = single_scroll_glyphs(src);
    assert_eq!(rs, vec![Glyph::SystemdService { unit: "onetwo".into() }]);
}

#[test]
fn non_string_interpolant_is_a_type_error() {
    // `port : Int` interpolated bare must fail at the `String.concat` site.
    let src = r#"
port = 8080
main = [ scroll { name = "test", glyphs = [ systemdService { unit = "${port}" } ] } ]
"#;
    assert_eq!(err_phase(src), emet::Phase::Type);
}

#[test]
fn maybe_interpolant_is_a_type_error() {
    let src = r#"
x = "v"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = "${Just x}" } ] } ]
"#;
    assert_eq!(err_phase(src), emet::Phase::Type);
}

#[test]
fn escaped_dollar_brace_is_a_literal() {
    // `\${` must not open an interpolation; it is the two literal chars `${`.
    let rs = glyphs(r#"[ systemdService { unit = "raw \${x}" } ]"#);
    assert_eq!(rs, vec![Glyph::SystemdService { unit: "raw ${x}".into() }]);
}

#[test]
fn embedded_expression_containing_braces() {
    // A record literal + field access inside `${ … }` proves brace-depth
    // matching: the record's `{`/`}` must not close the interpolation.
    let src = r#"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = "${ { host = "web" }.host }.service" } ] } ]
"#;
    let rs = single_scroll_glyphs(src);
    assert_eq!(rs, vec![Glyph::SystemdService { unit: "web.service".into() }]);
}

#[test]
fn embedded_case_with_layout_arms() {
    // A `case` inside `${ … }` laid out across lines lexes and parses; the
    // interpolation's `}` still closes on the matching brace.
    let src = "flag = True\nchoose f =\n  case f of\n    True -> \"on\"\n    False -> \"off\"\nmain = [ scroll { name = \"test\", glyphs = [ systemdService { unit = \"${choose flag}\" } ] } ]\n";
    let rs = single_scroll_glyphs(src);
    assert_eq!(rs, vec![Glyph::SystemdService { unit: "on".into() }]);
}

#[test]
fn nested_string_literal_inside_interpolation() {
    // The `"}"` inside `${ … }` must be skipped when scanning for the closing
    // `}`; a nested string's brace does not count.
    let src = r#"
x = "a"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = "${ String.append x "}" }" } ] } ]
"#;
    let rs = single_scroll_glyphs(src);
    assert_eq!(rs, vec![Glyph::SystemdService { unit: "a}".into() }]);
}

#[test]
fn nested_interpolated_string_inside_interpolation() {
    let src = r#"
x = "a"
y = "b"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = "${ String.append x "-${y}" }" } ] } ]
"#;
    let rs = single_scroll_glyphs(src);
    assert_eq!(rs, vec![Glyph::SystemdService { unit: "a-b".into() }]);
}

#[test]
fn plain_non_interpolated_string_still_works() {
    let rs = glyphs(r#"[ systemdService { unit = "plain.service" } ]"#);
    assert_eq!(rs, vec![Glyph::SystemdService { unit: "plain.service".into() }]);
}

#[test]
fn interpolation_desugars_to_string_concat_of_appends() {
    // Whole-expression interpolation composes with ordinary `++`.
    let src = r#"
a = "left"
b = "right"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = "${a}" ++ "-" ++ "${b}" } ] } ]
"#;
    let rs = single_scroll_glyphs(src);
    assert_eq!(rs, vec![Glyph::SystemdService { unit: "left-right".into() }]);
}
