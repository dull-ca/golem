mod common;
use common::err;
use emet::{compile, ir::Glyph, Phase};

#[test]
fn literal_base_update() {
    let src = r#"
main = [ scroll { name = { { name = "a", port = "1" } | name = "b" }.name, glyphs = [] } ]
"#;
    let c = compile(src).expect("updating a record literal should compile");
    assert_eq!(c.scrolls[0].name, "b");
}

#[test]
fn variable_base_update() {
    let src = r#"
base = { name = "a", port = "1" }
main = [ scroll { name = { base | name = "b" }.name, glyphs = [] } ]
"#;
    let c = compile(src).expect("updating a bound record should compile");
    assert_eq!(c.scrolls[0].name, "b");
}

#[test]
fn untouched_fields_survive_the_update() {
    let src = r#"
base = { name = "a", port = "1" }
main = [ scroll { name = { base | name = "b" }.port, glyphs = [] } ]
"#;
    let c = compile(src).expect("an untouched field should survive the update");
    assert_eq!(c.scrolls[0].name, "1");
}

#[test]
fn several_fields_at_once() {
    let src = r#"
base = { name = "a", unit = "u", extra = "e" }
updated = { base | name = "b", unit = "v" }
main =
  [ scroll
      { name = updated.name
      , glyphs =
          [ systemdService { unit = updated.unit }
          , lineInFile { path = "/p", line = updated.extra }
          ]
      }
  ]
"#;
    let c = compile(src).expect("a multi-field update should compile");
    assert_eq!(c.scrolls[0].name, "b");
    assert_eq!(
        c.scrolls[0].glyphs(),
        [
            Glyph::SystemdService {
                unit: "v".to_string()
            },
            Glyph::LineInFile {
                path: "/p".to_string(),
                line: "e".to_string()
            }
        ]
    );
}

#[test]
fn update_may_not_change_a_field_type() {
    let src = r#"
base : { name : String, port : Int }
base = { name = "a", port = 1 }
main = [ scroll { name = { base | port = "eighty" }.name, glyphs = [] } ]
"#;
    let e = err(src);
    assert_eq!(e.phase, Phase::Type);
    assert!(
        e.msg.contains("port"),
        "the message must name the field whose type would change, got: {}",
        e.msg
    );
    assert!(
        e.msg.contains("Int") && e.msg.contains("String"),
        "the message must name both the field's type and the new value's, got: {}",
        e.msg
    );
}

#[test]
fn update_may_not_change_an_inferred_field_type() {
    let src = r#"
base = { name = "a", port = 1 }
main = [ scroll { name = { base | port = "eighty" }.name, glyphs = [] } ]
"#;
    assert_eq!(err(src).phase, Phase::Type);
}

#[test]
fn update_through_a_row_polymorphic_parameter() {
    let src = r#"
rename n r = { r | name = n }
main =
  [ scroll { name = (rename "b" { name = "a", port = "1" }).name, glyphs = [] }
  , scroll { name = (rename "d" { name = "c", tag = 1, extra = "e" }).name, glyphs = [] }
  ]
"#;
    let c = compile(src).expect("one setter should serve two record shapes");
    assert_eq!(c.scrolls.len(), 2);
    assert_eq!(c.scrolls[0].name, "b");
    assert_eq!(c.scrolls[1].name, "d");
}

#[test]
fn a_row_polymorphic_setter_still_preserves_the_field_type() {
    let src = r#"
rename n r = { r | name = n }
main =
  [ scroll
      { name = String.fromInt (rename 1 { name = "a", port = "1" }).name
      , glyphs = []
      }
  ]
"#;
    let e = err(src);
    assert_eq!(e.phase, Phase::Type);
    assert!(
        e.msg.contains("String"),
        "the setter must keep `name` a `String`, got: {}",
        e.msg
    );
}

#[test]
fn updating_an_absent_field_names_it() {
    let src = r#"
main = [ scroll { name = { { name = "a", port = "1" } | prt = "2" }.name, glyphs = [] } ]
"#;
    let e = err(src);
    assert_eq!(e.phase, Phase::Type);
    assert!(
        e.msg.contains("prt"),
        "the message must name the absent field, got: {}",
        e.msg
    );
    assert!(
        e.msg.contains("port"),
        "the message must suggest the field the author meant, got: {}",
        e.msg
    );
    let note = e.note.expect("the error should carry a note");
    assert!(
        note.contains("`name`") && note.contains("`port`"),
        "the note must list the fields the record does have, got: {note}"
    );
}

#[test]
fn updating_a_non_record_is_a_type_error() {
    let src = r#"
main = [ scroll { name = { "not-a-record" | name = "b" }.name, glyphs = [] } ]
"#;
    let e = err(src);
    assert_eq!(e.phase, Phase::Type);
    assert!(
        e.msg.contains("String"),
        "the message must name the offending type, got: {}",
        e.msg
    );
}

#[test]
fn an_absent_field_is_underlined_at_its_name() {
    let src = "main = [ scroll { name = { { name = \"a\" } | prt = \"2\" }.name, glyphs = [] } ]\n";
    let e = err(src);
    assert_eq!(e.phase, Phase::Type);
    assert_eq!(
        &src[e.span.clone()],
        "prt",
        "the caret must sit on the offending field name, not its value"
    );
}

#[test]
fn an_update_with_no_fields_is_a_parse_error() {
    let src = r#"
base = { name = "a" }
main = [ scroll { name = { base | }.name, glyphs = [] } ]
"#;
    assert_eq!(err(src).phase, Phase::Parse);
}

#[test]
fn a_single_bar_is_update_syntax_not_an_infix_operator() {
    let src = r#"
x = 1 | 2
main = [ scroll { name = "a", glyphs = [] } ]
"#;
    assert_eq!(
        err(src).phase,
        Phase::Parse,
        "`|` must never gain a fixity — record update reads it as syntax"
    );
}

#[test]
fn a_double_bar_is_still_the_or_operator() {
    let src = r#"
main = [ scroll { name = if True || False then "y" else "n", glyphs = [] } ]
"#;
    let c = compile(src).expect("`||` is a distinct token and must keep its fixity");
    assert_eq!(c.scrolls[0].name, "y");
}
