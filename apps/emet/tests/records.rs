//! Row-polymorphic records: field access on a lambda/parameter now infers an
//! open record row, so a helper that reads `h.name` type-checks and composes.

mod common;
use common::{err, single_scroll_glyphs};
use emet::{compile, ir::Glyph, Phase};

// A bare field-access lambda is usable and polymorphic over the record's rest.
#[test]
fn field_access_lambda_type_checks() {
    let src = r#"
getName h = h.name
main = [ scroll { name = getName { name = "web1", port = 8080 }, glyphs = [] } ]
"#;
    let c = compile(src).expect("field-access lambda should type-check");
    assert_eq!(c.scrolls.len(), 1);
    assert_eq!(c.scrolls[0].name, "web1");
}

// A helper with a closed record signature reading two fields off its param.
#[test]
fn helper_with_record_signature() {
    let src = r#"
mk : { name : String, port : Int } -> Scroll
mk h = scroll { name = h.name, glyphs = [ systemdService { unit = h.name } ] }
main = [ mk { name = "web1", port = 8080 } ]
"#;
    let c = compile(src).expect("record-signature helper should compile");
    assert_eq!(c.scrolls[0].name, "web1");
}

// The README-style pattern: map a field-reading lambda over a list of records.
#[test]
fn record_map_builds_list_scroll() {
    let src = r#"
hostScroll name = scroll { name = name, glyphs = [ systemdService { unit = name } ] }
main = List.map (\h -> hostScroll h.name) [ { name = "web1", port = 8080 }, { name = "web2", port = 9090 } ]
"#;
    let c = compile(src).expect("record-map pattern should compile");
    assert_eq!(c.scrolls.len(), 2);
    assert_eq!(c.scrolls[0].name, "web1");
    assert_eq!(c.scrolls[1].name, "web2");
}

// Two distinct record shapes flowing into the same polymorphic helper.
#[test]
fn polymorphic_over_record_shape() {
    let src = r#"
getName h = h.name
main =
  [ scroll { name = getName { name = "a", port = 1 }, glyphs = [] }
  , scroll { name = getName { name = "b", tag = "x" }, glyphs = [] }
  ]
"#;
    let c = compile(src).expect("one helper at two record shapes should compile");
    assert_eq!(c.scrolls[0].name, "a");
    assert_eq!(c.scrolls[1].name, "b");
}

// An extra-field record satisfies an open row demanding fewer fields.
#[test]
fn extra_fields_pass_open_row() {
    let src = r#"
getPort h = h.port
main = [ scroll { name = "x", glyphs = [ lineInFile { path = "/p", line = getPort { port = "9", extra = "y" } } ] } ]
"#;
    let rs = single_scroll_glyphs(src);
    assert_eq!(
        rs[0],
        Glyph::LineInFile {
            path: "/p".into(),
            line: "9".into(),
            perms: None
        }
    );
}

// A closed signature still constrains exactly: an extra field is rejected.
#[test]
fn closed_signature_rejects_extra_field() {
    let src = r#"
mk : { name : String } -> Scroll
mk h = scroll { name = h.name, glyphs = [] }
main = [ mk { name = "web1", port = 8080 } ]
"#;
    let e = err(src);
    assert_eq!(e.phase, Phase::Type);
}

// Field access on a non-record is a clear type error.
#[test]
fn field_access_on_non_record_is_type_error() {
    let src = r#"
bad h = h.name
main = [ scroll { name = bad "not-a-record", glyphs = [] } ]
"#;
    let e = err(src);
    assert_eq!(e.phase, Phase::Type);
}

// Accessing a field a closed record lacks is a type error.
#[test]
fn missing_demanded_field_is_type_error() {
    let src = r#"
main = [ scroll { name = { name = "x" }.port, glyphs = [] } ]
"#;
    let e = err(src);
    assert_eq!(e.phase, Phase::Type);
}
