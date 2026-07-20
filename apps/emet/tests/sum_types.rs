//! Sum-type constructors as first-class values, the built-in `Maybe` and
//! `Bool` types, and the `Maybe.*` builtins. No `case`/`if` yet — the
//! builtins make `Maybe` usable without an elimination form.

mod common;

use common::{err_phase, glyphs, single_scroll_glyphs};
use emet::ir::Glyph;

#[test]
fn constructors_type_check_against_signatures() {
    let src = r#"
just : Maybe String
just = Just "x"
nothing : Maybe a
nothing = Nothing
yes : Bool
yes = True
no : Bool
no = False
main = [ scroll { name = "test", glyphs = [ ] } ]
"#;
    assert!(single_scroll_glyphs(src).is_empty());
}

#[test]
fn maybe_with_default_uses_just_payload() {
    let rs = glyphs(
        r#"[ systemdService { unit = Maybe.withDefault "fallback.service" (Just "nginx.service") } ]"#,
    );
    assert_eq!(rs, vec![Glyph::SystemdService { unit: "nginx.service".into() }]);
}

#[test]
fn maybe_with_default_falls_back_on_nothing() {
    let src = r#"
missing : Maybe String
missing = Nothing
main = [ scroll { name = "test", glyphs = [ systemdService { unit = Maybe.withDefault "fallback.service" missing } ] } ]
"#;
    assert_eq!(
        single_scroll_glyphs(src),
        vec![Glyph::SystemdService { unit: "fallback.service".into() }]
    );
}

#[test]
fn maybe_map_transforms_the_payload() {
    // The payload is mapped from a `String` to a unit string, then re-wrapped;
    // `withDefault` observes the mapped `Just`.
    let src = r#"
mapped = Maybe.map (\u -> systemdService { unit = u }) (Just "nginx.service")
main = [ scroll { name = "test", glyphs = [ Maybe.withDefault (systemdService { unit = "unused.service" }) mapped ] } ]
"#;
    assert_eq!(
        single_scroll_glyphs(src),
        vec![Glyph::SystemdService { unit: "nginx.service".into() }]
    );
}

#[test]
fn maybe_and_then_chains_through_just() {
    let src = r#"
step u = Just (systemdService { unit = u })
chained = Maybe.andThen step (Just "nginx.service")
main = [ scroll { name = "test", glyphs = [ Maybe.withDefault (systemdService { unit = "unused.service" }) chained ] } ]
"#;
    assert_eq!(
        single_scroll_glyphs(src),
        vec![Glyph::SystemdService { unit: "nginx.service".into() }]
    );
}

#[test]
fn maybe_and_then_short_circuits_on_nothing() {
    let src = r#"
step u = Just (systemdService { unit = u })
start : Maybe String
start = Nothing
chained = Maybe.andThen step start
main = [ scroll { name = "test", glyphs = [ Maybe.withDefault (systemdService { unit = "default.service" }) chained ] } ]
"#;
    assert_eq!(
        single_scroll_glyphs(src),
        vec![Glyph::SystemdService { unit: "default.service".into() }]
    );
}

#[test]
fn unknown_constructor_is_a_type_error() {
    let src = r#"[ Foo "x" ]"#;
    assert_eq!(err_phase(&common::one_scroll(src)), emet::Phase::Type);
}
