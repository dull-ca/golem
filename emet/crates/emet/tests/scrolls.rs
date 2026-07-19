//! The `Scroll` per-host container (ADR 0009): `main : List Scroll`, per-scroll
//! glyph grouping, per-scroll conflict analysis, interpolated scroll names, and
//! the shift of the program's output bottom from `List Glyph` to `List Scroll`.

mod common;

use common::err;
use emet::{compile, ir::Glyph, ir::Scroll, Phase};

fn scrolls(src: &str) -> Vec<Scroll> {
    match compile(src) {
        Ok(c) => c.scrolls,
        Err(e) => panic!("expected success, got {:?}: {}", e.phase, e.msg),
    }
}

#[test]
fn multi_scroll_fleet_groups_glyphs_per_scroll_in_order() {
    let src = r#"
main =
  [ scroll { name = "web", glyphs = [ aptPackage { name = "nginx" }, systemdService { unit = "nginx.service" } ] }
  , scroll { name = "db", glyphs = [ aptPackage { name = "postgresql" } ] }
  ]
"#;
    let ss = scrolls(src);
    assert_eq!(ss.len(), 2);
    assert_eq!(
        ss[0],
        Scroll {
            name: "web".into(),
            glyphs: vec![
                Glyph::AptPackage { name: "nginx".into() },
                Glyph::SystemdService { unit: "nginx.service".into() },
            ],
        }
    );
    assert_eq!(
        ss[1],
        Scroll {
            name: "db".into(),
            glyphs: vec![Glyph::AptPackage { name: "postgresql".into() }],
        }
    );
}

#[test]
fn two_scrolls_sharing_a_glyph_key_do_not_conflict() {
    // The same install on two different hosts is legitimate, not a conflict.
    let src = r#"
main =
  [ scroll { name = "web-1", glyphs = [ aptPackage { name = "nginx" } ] }
  , scroll { name = "web-2", glyphs = [ aptPackage { name = "nginx" } ] }
  ]
"#;
    let ss = scrolls(src);
    assert_eq!(ss.len(), 2);
    assert_eq!(ss[0].glyphs, ss[1].glyphs);
}

#[test]
fn conflicting_glyph_keys_within_one_scroll_is_analyze_error() {
    let src = r#"
main =
  [ scroll
      { name = "web"
      , glyphs =
          [ file { path = "/etc/motd", contents = "hi", mode = "0644" }
          , file { path = "/etc/motd", contents = "bye", mode = "0644" }
          ]
      }
  ]
"#;
    let e = err(src);
    assert_eq!(e.phase, Phase::Analyze);
    assert!(e.msg.contains("file:/etc/motd"), "got: {}", e.msg);
}

#[test]
fn interpolated_scroll_name() {
    let src = r#"
n = 3
main = [ scroll { name = "web-${String.fromInt n}", glyphs = [ aptPackage { name = "nginx" } ] } ]
"#;
    let ss = scrolls(src);
    assert_eq!(ss.len(), 1);
    assert_eq!(ss[0].name, "web-3");
}

#[test]
fn main_as_list_glyph_is_now_a_type_error() {
    // The output bottom shifted to `List Scroll`, so a bare glyph list `main` is
    // no longer a valid program.
    let src = r#"main = [ aptPackage { name = "nginx" } ]"#;
    let e = err(src);
    assert_eq!(e.phase, Phase::Type);
    assert!(e.msg.contains("List Scroll"), "got: {}", e.msg);
}

#[test]
fn scroll_missing_field_is_a_parse_error() {
    let e = err(r#"main = [ scroll { name = "web" } ]"#);
    assert_eq!(e.phase, Phase::Parse);
    assert!(e.msg.contains("`scroll` requires a `glyphs` field"), "got: {}", e.msg);
}

#[test]
fn scroll_unknown_field_is_a_parse_error() {
    let e = err(r#"main = [ scroll { name = "web", glyphs = [ ], ipv4 = "10.0.0.1" } ]"#);
    assert_eq!(e.phase, Phase::Parse);
    assert!(e.msg.contains("unknown scroll field `ipv4`"), "got: {}", e.msg);
}

#[test]
fn scroll_glyphs_field_must_be_a_glyph_list() {
    // A String is not a `List Glyph`, so the `glyphs` field rejects it.
    let src = r#"main = [ scroll { name = "web", glyphs = "nope" } ]"#;
    let e = err(src);
    assert_eq!(e.phase, Phase::Type);
}
