//! End-to-end tests: source -> glyphs, plus type-error checks.

mod common;

use common::{err_phase, glyphs, single_scroll_glyphs};
use emet::{compile, ir::Glyph, Phase};

#[test]
fn minimal_apt_package() {
    let rs = glyphs(r#"[ aptPackage { name = "nginx" } ]"#);
    assert_eq!(rs, vec![Glyph::AptPackage { name: "nginx".into() }]);
}

#[test]
fn minimal_systemd_service() {
    let rs = glyphs(r#"[ systemdService { unit = "nginx.service" } ]"#);
    assert_eq!(rs, vec![Glyph::SystemdService { unit: "nginx.service".into() }]);
}

#[test]
fn mixed_glyph_list_checks_and_orders() {
    let rs = glyphs(
        r#"[ aptPackage { name = "nginx" }, systemdService { unit = "nginx.service" } ]"#,
    );
    assert_eq!(
        rs,
        vec![
            Glyph::AptPackage { name: "nginx".into() },
            Glyph::SystemdService { unit: "nginx.service".into() },
        ]
    );
}

#[test]
fn single_line_let_uses_parse_error_rule() {
    // The implicit block opened after `let` must be closed by the parse error
    // on `in`. If parse-error(t) is broken, this fails to parse.
    let rs = single_scroll_glyphs(
        r#"main = let u = "u.service" in [ scroll { name = "test", glyphs = [ systemdService { unit = u } ] } ]"#,
    );
    assert_eq!(rs.len(), 1);
    assert_eq!(rs[0], Glyph::SystemdService { unit: "u.service".into() });
}

#[test]
fn user_defined_abstraction_via_function() {
    let src = r#"
webserver name = aptPackage { name = name }
main = [ scroll { name = "test", glyphs = [ webserver "nginx", webserver "openresty" ] } ]
"#;
    let rs = single_scroll_glyphs(src);
    assert_eq!(rs.len(), 2);
    assert_eq!(rs[0].key(), "apt:nginx");
    assert_eq!(rs[1].key(), "apt:openresty");
}

#[test]
fn hm_infers_polymorphic_identity() {
    // id is used at two types; generalization must make this check.
    let src = r#"
id x = x
main = [ scroll { name = "test", glyphs = [ id (aptPackage { name = id "p" }) ] } ]
"#;
    let rs = single_scroll_glyphs(src);
    assert_eq!(rs.len(), 1);
}

#[test]
fn signature_agreement_ok() {
    let src = r#"
webserver : Str -> SystemdService
webserver unit = systemdService { unit = unit }
main : List Scroll
main = [ scroll { name = "test", glyphs = [ webserver "nginx.service" ] } ]
"#;
    assert_eq!(single_scroll_glyphs(src).len(), 1);
}

#[test]
fn concrete_glyph_type_in_signature_checks() {
    let src = r#"
basePkg : Str -> AptPackage
basePkg name = aptPackage { name = name }
main = [ scroll { name = "test", glyphs = [ basePkg "nginx" ] } ]
"#;
    let rs = single_scroll_glyphs(src);
    assert_eq!(rs, vec![Glyph::AptPackage { name: "nginx".into() }]);
}

#[test]
fn signature_conflict_is_type_error() {
    // Declared SystemdService but body is a function -> type error.
    let src = r#"
webserver : SystemdService
webserver unit = systemdService { unit = unit }
main = [ scroll { name = "test", glyphs = [ webserver "x" ] } ]
"#;
    assert_eq!(err_phase(src), Phase::Type);
}

#[test]
fn wrong_field_type_is_type_error() {
    // A glyph is not a Str, so it cannot fill the `unit` field.
    let src = r#"main = [ scroll { name = "test", glyphs = [ systemdService { unit = aptPackage { name = "x" } } ] } ]"#;
    assert_eq!(err_phase(src), Phase::Type);
}

#[test]
fn unknown_name_is_type_error() {
    let src = r#"main = [ scroll { name = "test", glyphs = [ nope ] } ]"#;
    assert_eq!(err_phase(src), Phase::Type);
}

#[test]
fn missing_main_is_type_error() {
    assert_eq!(err_phase(r#"foo = "bar""#), Phase::Type);
}

#[test]
fn main_type_renders_as_canonical_spelling() {
    let src = r#"
webserver : Str -> SystemdService
webserver unit = systemdService { unit = unit }
main : List Scroll
main = [ scroll { name = "test", glyphs = [ webserver "nginx.service" ] } ]
"#;
    let c = compile(src).expect("aliases should compile");
    assert_eq!(c.main_ty.to_string(), "List Scroll");
}

#[test]
fn record_field_access() {
    let src = r#"
cfg = { pkg = "nginx", unit = "nginx.service" }
main = [ scroll { name = "test", glyphs = [ aptPackage { name = cfg.pkg }, systemdService { unit = cfg.unit } ] } ]
"#;
    let rs = single_scroll_glyphs(src);
    assert_eq!(rs[0], Glyph::AptPackage { name: "nginx".into() });
    assert_eq!(rs[1], Glyph::SystemdService { unit: "nginx.service".into() });
}

#[test]
fn conflicting_declarations_flagged() {
    // Identical duplicates share a key with identical content, so they are
    // idempotent and analysis passes.
    let src = r#"
w name = aptPackage { name = name }
main = [ scroll { name = "test", glyphs = [ w "p", w "p" ] } ]
"#;
    assert_eq!(single_scroll_glyphs(src).len(), 2);
}

#[test]
fn nested_let_multiline() {
    let src = "main =\n  [ scroll\n      { name = \"test\"\n      , glyphs =\n          let a = \"pkg\"\n              u = \"pkg.service\"\n          in [ aptPackage { name = a }, systemdService { unit = u } ]\n      }\n  ]\n";
    let rs = single_scroll_glyphs(src);
    assert_eq!(rs.len(), 2);
    assert_eq!(rs[0], Glyph::AptPackage { name: "pkg".into() });
    assert_eq!(rs[1], Glyph::SystemdService { unit: "pkg.service".into() });
}
