//! The `file` and `lineInFile` glyph primitives: end-to-end construction,
//! interpolated (concrete-string) contents, field errors, mixed lists, and the
//! `analyze` dedup/conflict behavior keyed by `Glyph::key`.

mod common;

use common::{err, glyphs, single_scroll_glyphs};
use emet::{ir::Glyph, Phase};

#[test]
fn file_produces_a_file_glyph() {
    let rs = glyphs(r#"[ file { path = "/etc/nginx.conf", contents = "listen 80;", mode = "0644" } ]"#);
    assert_eq!(
        rs,
        vec![Glyph::File {
            path: "/etc/nginx.conf".into(),
            contents: "listen 80;".into(),
            mode: "0644".into(),
        }]
    );
}

#[test]
fn line_in_file_produces_a_line_in_file_glyph() {
    let rs = glyphs(r#"[ lineInFile { path = "/etc/hosts", line = "127.0.0.1 local" } ]"#);
    assert_eq!(
        rs,
        vec![Glyph::LineInFile {
            path: "/etc/hosts".into(),
            line: "127.0.0.1 local".into(),
        }]
    );
}

#[test]
fn interpolated_contents_arrive_as_a_concrete_string() {
    // The language is the generator: the interpolation lowers to String.concat
    // upstream, so the IR never sees a template — only the evaluated String.
    let src = r#"
port = 8080
main = [ scroll { name = "test", glyphs = [ file { path = "/etc/svc.conf", contents = "listen ${String.fromInt port};", mode = "0644" } ] } ]
"#;
    let rs = single_scroll_glyphs(src);
    assert_eq!(
        rs,
        vec![Glyph::File {
            path: "/etc/svc.conf".into(),
            contents: "listen 8080;".into(),
            mode: "0644".into(),
        }]
    );
}

#[test]
fn file_missing_field_is_a_parse_error() {
    let e = err(r#"main = [ scroll { name = "test", glyphs = [ file { path = "/x", mode = "0644" } ] } ]"#);
    assert_eq!(e.phase, Phase::Parse);
    assert!(e.msg.contains("`file` requires a `contents` field"), "got: {}", e.msg);
}

#[test]
fn file_unknown_field_is_a_parse_error() {
    let e = err(r#"main = [ scroll { name = "test", glyphs = [ file { path = "/x", contents = "c", mode = "0644", owner = "root" } ] } ]"#);
    assert_eq!(e.phase, Phase::Parse);
    assert!(e.msg.contains("unknown file field `owner`"), "got: {}", e.msg);
}

#[test]
fn line_in_file_missing_field_is_a_parse_error() {
    let e = err(r#"main = [ scroll { name = "test", glyphs = [ lineInFile { path = "/x" } ] } ]"#);
    assert_eq!(e.phase, Phase::Parse);
    assert!(e.msg.contains("`lineInFile` requires a `line` field"), "got: {}", e.msg);
}

#[test]
fn line_in_file_unknown_field_is_a_parse_error() {
    let e = err(r#"main = [ scroll { name = "test", glyphs = [ lineInFile { path = "/x", line = "l", when = "always" } ] } ]"#);
    assert_eq!(e.phase, Phase::Parse);
    assert!(e.msg.contains("unknown lineInFile field `when`"), "got: {}", e.msg);
}

#[test]
fn mixed_glyph_list_checks_and_orders() {
    let src = r#"
main =
  [ scroll
      { name = "test"
      , glyphs =
          [ aptPackage { name = "nginx" }
          , file { path = "/etc/nginx.conf", contents = "listen 80;", mode = "0644" }
          , lineInFile { path = "/etc/hosts", line = "127.0.0.1 local" }
          ]
      }
  ]
"#;
    let rs = single_scroll_glyphs(src);
    assert_eq!(
        rs,
        vec![
            Glyph::AptPackage { name: "nginx".into() },
            Glyph::File {
                path: "/etc/nginx.conf".into(),
                contents: "listen 80;".into(),
                mode: "0644".into(),
            },
            Glyph::LineInFile {
                path: "/etc/hosts".into(),
                line: "127.0.0.1 local".into(),
            },
        ]
    );
}

#[test]
fn identical_files_at_one_path_dedup() {
    let src = r#"
f = file { path = "/etc/motd", contents = "hi", mode = "0644" }
main = [ scroll { name = "test", glyphs = [ f, f ] } ]
"#;
    assert_eq!(single_scroll_glyphs(src).len(), 2);
}

#[test]
fn different_files_at_one_path_conflict() {
    let src = r#"
main =
  [ scroll
      { name = "test"
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
