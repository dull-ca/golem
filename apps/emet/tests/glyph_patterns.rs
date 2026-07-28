//! Glyph and filesystem-entry pattern matching (ADR 0017 / 0019): the four
//! `Glyph` variants and the three `Entry` variants are ordinary nominal sums,
//! matched by their PascalCase tag with exhaustiveness checking, while still
//! built by their lowercase reserved word. The directed-widening replacement of
//! ADR 0002's symmetric injection is exercised too: concretes still collect into
//! `List Glyph`, but a `Glyph` cannot be used where a concrete glyph is required.

mod common;

use emet::{ir::Glyph, Phase};

fn unit(src: &str) -> String {
    match common::single_scroll_glyphs(src).as_slice() {
        [Glyph::SystemdService { unit }] => unit.clone(),
        other => panic!("expected a single systemdService glyph, got {other:?}"),
    }
}

fn err(src: &str) -> (Phase, String) {
    let e = common::err(src);
    (e.phase, e.msg)
}

#[test]
fn case_binds_apt_package_name() {
    let src = r#"
label g =
  case g of
    AptPackage p -> p.name
    SystemdService s -> s.unit
    Filesystem f -> f.path
    LineInFile l -> l.line
main = [ scroll { name = "test", glyphs = [ systemdService { unit = label (aptPackage { name = "nginx" }) } ] } ]
"#;
    assert_eq!(unit(src), "nginx");
}

#[test]
fn case_binds_systemd_unit() {
    let src = r#"
label g =
  case g of
    AptPackage p -> p.name
    SystemdService s -> s.unit
    Filesystem f -> f.path
    LineInFile l -> l.line
main = [ scroll { name = "test", glyphs = [ systemdService { unit = label (systemdService { unit = "sshd.service" }) } ] } ]
"#;
    assert_eq!(unit(src), "sshd.service");
}

#[test]
fn case_binds_filesystem_path() {
    let src = r#"
label g =
  case g of
    AptPackage p -> p.name
    SystemdService s -> s.unit
    Filesystem f -> f.path
    LineInFile l -> l.line
main = [ scroll { name = "test", glyphs = [ systemdService { unit = label (file { path = "/etc/motd", contents = "hi", mode = "0644" }) } ] } ]
"#;
    assert_eq!(unit(src), "/etc/motd");
}

#[test]
fn case_binds_line_in_file_line() {
    let src = r#"
label g =
  case g of
    AptPackage p -> p.name
    SystemdService s -> s.unit
    Filesystem f -> f.path
    LineInFile l -> l.line
main = [ scroll { name = "test", glyphs = [ systemdService { unit = label (lineInFile { path = "/etc/hosts", line = "127.0.0.1 host" }) } ] } ]
"#;
    assert_eq!(unit(src), "127.0.0.1 host");
}

#[test]
fn missing_glyph_variant_is_non_exhaustive() {
    let src = r#"
label g =
  case g of
    AptPackage p -> p.name
    SystemdService s -> s.unit
    Filesystem f -> f.path
main = [ scroll { name = "test", glyphs = [ systemdService { unit = label (aptPackage { name = "nginx" }) } ] } ]
"#;
    assert_eq!(err(src).0, Phase::Type);
}

#[test]
fn wildcard_covers_remaining_glyph_variants() {
    let src = r#"
isApt g =
  case g of
    AptPackage p -> "yes"
    _ -> "no"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = isApt (systemdService { unit = "x" }) } ] } ]
"#;
    assert_eq!(unit(src), "no");
}

#[test]
fn redundant_glyph_arm_after_catch_all() {
    let src = r#"
isApt g =
  case g of
    _ -> "any"
    AptPackage p -> "apt"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = isApt (aptPackage { name = "x" }) } ] } ]
"#;
    assert_eq!(err(src).0, Phase::Type);
}

#[test]
fn filesystem_entry_matches_file_arm() {
    let src = r#"
describeEntry g =
  case g of
    Filesystem f ->
      case f.entry of
        File contents -> contents.contents
        Directory d -> "dir"
        Symlink s -> s.target
    _ -> "other"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = describeEntry (file { path = "/a", contents = "body", mode = "0644" }) } ] } ]
"#;
    assert_eq!(unit(src), "body");
}

#[test]
fn filesystem_entry_matches_symlink_arm() {
    let src = r#"
describeEntry g =
  case g of
    Filesystem f ->
      case f.entry of
        File contents -> contents.contents
        Directory d -> "dir"
        Symlink s -> s.target
    _ -> "other"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = describeEntry (symlink { path = "/a", target = "/b" }) } ] } ]
"#;
    assert_eq!(unit(src), "/b");
}

#[test]
fn filesystem_entry_matches_directory_arm() {
    let src = r#"
describeEntry g =
  case g of
    Filesystem f ->
      case f.entry of
        File contents -> contents.contents
        Directory d -> "dir"
        Symlink s -> s.target
    _ -> "other"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = describeEntry (directory { path = "/a", mode = "0755" }) } ] } ]
"#;
    assert_eq!(unit(src), "dir");
}

#[test]
fn missing_entry_variant_is_non_exhaustive() {
    let src = r#"
describeEntry g =
  case g of
    Filesystem f ->
      case f.entry of
        File contents -> contents.contents
        Directory d -> "dir"
    _ -> "other"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = describeEntry (file { path = "/a", contents = "x", mode = "0644" }) } ] } ]
"#;
    assert_eq!(err(src).0, Phase::Type);
}

#[test]
fn file_perms_mode_is_an_int() {
    let src = r#"
modeOf g =
  case g of
    Filesystem f ->
      case f.entry of
        File contents -> String.fromInt contents.perms.mode
        Directory d -> String.fromInt d.perms.mode
        Symlink s -> "0"
    _ -> "none"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = modeOf (file { path = "/a", contents = "x", mode = "0644" }) } ] } ]
"#;
    assert_eq!(unit(src), "420");
}

#[test]
fn walk_scroll_glyphs_with_list_and_glyph_patterns() {
    let out = common::single_scroll_glyphs(
        r#"
firstName glyphs =
  case glyphs of
    [] -> "none"
    (g :: rest) ->
      case g of
        AptPackage p -> p.name
        SystemdService s -> s.unit
        Filesystem f -> f.path
        LineInFile l -> l.line
main =
  [ scroll { name = "test", glyphs = [ systemdService { unit = firstName [ aptPackage { name = "nginx" }, systemdService { unit = "sshd" } ] } ] } ]
"#,
    );
    match out.as_slice() {
        [Glyph::SystemdService { unit }] => assert_eq!(unit, "nginx"),
        other => panic!("got {other:?}"),
    }
}

#[test]
fn concretes_still_collect_into_a_glyph_list() {
    // Directed widening must still fire at the list join: a heterogeneous list of
    // concrete glyphs is a `List Glyph`.
    let src = r#"
gs = [ aptPackage { name = "nginx" }, systemdService { unit = "sshd" } ]
main = [ scroll { name = "test", glyphs = gs } ]
"#;
    let glyphs = common::single_scroll_glyphs(src);
    assert_eq!(glyphs.len(), 2);
}

#[test]
fn glyph_cannot_satisfy_a_concrete_glyph_requirement() {
    // Directed widening is one-way: a value typed `Glyph` (a lambda parameter
    // annotated `Glyph`) cannot flow into a position demanding a concrete
    // `AptPackage`. `scroll`'s glyphs field would need `List Glyph`, but forcing
    // the element back to a concrete via a `Filesystem`-typed hole must fail.
    let src = r#"
needsApt : AptPackage -> String
needsApt p = "ok"
firstName g =
  case g of
    AptPackage p -> needsApt g
    SystemdService s -> "s"
    Filesystem f -> "f"
    LineInFile l -> "l"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = firstName (aptPackage { name = "x" }) } ] } ]
"#;
    assert_eq!(err(src).0, Phase::Type);
}
