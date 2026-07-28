//! The filesystem glyph (`file`, `directory`, `symlink`) and the `lineInFile`
//! glyph: end-to-end construction, interpolated (concrete-string) contents,
//! per-arm field enforcement, mixed lists, and the `analyze` dedup/conflict
//! behavior keyed by `Glyph::key`.

mod common;

use common::{err, glyphs, single_scroll_glyphs};
use emet::{
    ir::{Entry, Glyph, Perms},
    Phase,
};

fn perms(mode: u16) -> Perms {
    Perms {
        mode,
        owner: None,
        group: None,
    }
}

#[test]
fn file_produces_a_filesystem_file_glyph() {
    let rs =
        glyphs(r#"[ file { path = "/etc/nginx.conf", contents = "listen 80;", mode = "0644" } ]"#);
    assert_eq!(
        rs,
        vec![Glyph::Filesystem {
            path: "/etc/nginx.conf".into(),
            entry: Entry::File {
                contents: "listen 80;".into(),
                perms: perms(0o644)
            },
        }]
    );
}

#[test]
fn directory_produces_a_filesystem_directory_glyph() {
    let rs = glyphs(r#"[ directory { path = "/srv/registry/data", mode = "0755" } ]"#);
    assert_eq!(
        rs,
        vec![Glyph::Filesystem {
            path: "/srv/registry/data".into(),
            entry: Entry::Directory {
                perms: perms(0o755)
            },
        }]
    );
}

#[test]
fn symlink_produces_a_filesystem_symlink_glyph() {
    let rs = glyphs(
        r#"[ symlink { path = "/etc/nginx/sites-enabled/app", target = "/etc/nginx/sites-available/app" } ]"#,
    );
    assert_eq!(
        rs,
        vec![Glyph::Filesystem {
            path: "/etc/nginx/sites-enabled/app".into(),
            entry: Entry::Symlink {
                target: "/etc/nginx/sites-available/app".into()
            },
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
    let src = r#"
port = 8080
main = [ scroll { name = "test", glyphs = [ file { path = "/etc/svc.conf", contents = "listen ${String.fromInt port};", mode = "0644" } ] } ]
"#;
    let rs = single_scroll_glyphs(src);
    assert_eq!(
        rs,
        vec![Glyph::Filesystem {
            path: "/etc/svc.conf".into(),
            entry: Entry::File {
                contents: "listen 8080;".into(),
                perms: perms(0o644)
            },
        }]
    );
}

#[test]
fn file_missing_field_is_a_parse_error() {
    let e = err(
        r#"main = [ scroll { name = "test", glyphs = [ file { path = "/x", mode = "0644" } ] } ]"#,
    );
    assert_eq!(e.phase, Phase::Parse);
    assert!(
        e.msg.contains("`file` requires a `contents` field"),
        "got: {}",
        e.msg
    );
}

#[test]
fn file_unknown_field_is_a_parse_error() {
    let e = err(
        r#"main = [ scroll { name = "test", glyphs = [ file { path = "/x", contents = "c", mode = "0644", when = "always" } ] } ]"#,
    );
    assert_eq!(e.phase, Phase::Parse);
    assert!(
        e.msg.contains("unknown file field `when`"),
        "got: {}",
        e.msg
    );
}

#[test]
fn symlink_with_a_mode_is_a_parse_error() {
    let e = err(
        r#"main = [ scroll { name = "test", glyphs = [ symlink { path = "/x", target = "/y", mode = "0644" } ] } ]"#,
    );
    assert_eq!(e.phase, Phase::Parse);
    assert!(
        e.msg.contains("unknown symlink field `mode`"),
        "got: {}",
        e.msg
    );
}

#[test]
fn directory_with_contents_is_a_parse_error() {
    let e = err(
        r#"main = [ scroll { name = "test", glyphs = [ directory { path = "/x", mode = "0755", contents = "c" } ] } ]"#,
    );
    assert_eq!(e.phase, Phase::Parse);
    assert!(
        e.msg.contains("unknown directory field `contents`"),
        "got: {}",
        e.msg
    );
}

#[test]
fn directory_missing_mode_is_a_parse_error() {
    let e = err(r#"main = [ scroll { name = "test", glyphs = [ directory { path = "/x" } ] } ]"#);
    assert_eq!(e.phase, Phase::Parse);
    assert!(
        e.msg.contains("`directory` requires a `mode` field"),
        "got: {}",
        e.msg
    );
}

#[test]
fn symlink_missing_target_is_a_parse_error() {
    let e = err(r#"main = [ scroll { name = "test", glyphs = [ symlink { path = "/x" } ] } ]"#);
    assert_eq!(e.phase, Phase::Parse);
    assert!(
        e.msg.contains("`symlink` requires a `target` field"),
        "got: {}",
        e.msg
    );
}

#[test]
fn bad_mode_is_an_eval_error() {
    let e = err(
        r#"main = [ scroll { name = "test", glyphs = [ file { path = "/x", contents = "c", mode = "nope" } ] } ]"#,
    );
    assert_eq!(e.phase, Phase::Analyze);
    assert!(e.msg.contains("invalid mode `nope`"), "got: {}", e.msg);
}

#[test]
fn line_in_file_missing_field_is_a_parse_error() {
    let e = err(r#"main = [ scroll { name = "test", glyphs = [ lineInFile { path = "/x" } ] } ]"#);
    assert_eq!(e.phase, Phase::Parse);
    assert!(
        e.msg.contains("`lineInFile` requires a `line` field"),
        "got: {}",
        e.msg
    );
}

#[test]
fn line_in_file_unknown_field_is_a_parse_error() {
    let e = err(
        r#"main = [ scroll { name = "test", glyphs = [ lineInFile { path = "/x", line = "l", when = "always" } ] } ]"#,
    );
    assert_eq!(e.phase, Phase::Parse);
    assert!(
        e.msg.contains("unknown lineInFile field `when`"),
        "got: {}",
        e.msg
    );
}

#[test]
fn mixed_glyph_list_checks_and_orders() {
    let src = r#"
main =
  [ scroll
      { name = "test"
      , glyphs =
          [ aptPackage { name = "nginx" }
          , directory { path = "/srv/data", mode = "0755" }
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
            Glyph::AptPackage {
                name: "nginx".into()
            },
            Glyph::Filesystem {
                path: "/srv/data".into(),
                entry: Entry::Directory {
                    perms: perms(0o755)
                },
            },
            Glyph::Filesystem {
                path: "/etc/nginx.conf".into(),
                entry: Entry::File {
                    contents: "listen 80;".into(),
                    perms: perms(0o644)
                },
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

#[test]
fn a_directory_and_a_file_at_one_path_conflict() {
    let src = r#"
main =
  [ scroll
      { name = "test"
      , glyphs =
          [ file { path = "/srv/x", contents = "hi", mode = "0644" }
          , directory { path = "/srv/x", mode = "0755" }
          ]
      }
  ]
"#;
    let e = err(src);
    assert_eq!(e.phase, Phase::Analyze);
    assert!(e.msg.contains("file:/srv/x"), "got: {}", e.msg);
}
