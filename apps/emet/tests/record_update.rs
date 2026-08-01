mod common;
use common::err;
use emet::{compile, Phase};

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
    assert_eq!(c.scrolls[0].glyphs().len(), 2);
}

#[test]
fn update_may_change_a_field_type() {
    let src = r#"
base = { name = "a", port = 1 }
relabeled = { base | port = "eighty" }
main =
  [ scroll
      { name = base.name
      , glyphs =
          [ lineInFile { path = "/p", line = relabeled.port }
          , lineInFile { path = "/q", line = String.fromInt base.port }
          ]
      }
  ]
"#;
    let c = compile(src).expect("a field's type may change where the rows allow it");
    assert_eq!(c.scrolls[0].glyphs().len(), 2);
}

#[test]
fn update_through_a_row_polymorphic_parameter() {
    let src = r#"
rename n r = { r | name = n }
main = [ scroll { name = (rename "b" { name = "a", port = "1" }).name, glyphs = [] } ]
"#;
    let c = compile(src).expect("a row-polymorphic setter should compile");
    assert_eq!(c.scrolls[0].name, "b");
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
fn an_update_with_no_fields_is_a_parse_error() {
    let src = r#"
base = { name = "a" }
main = [ scroll { name = { base | }.name, glyphs = [] } ]
"#;
    assert_eq!(err(src).phase, Phase::Parse);
}
