//! `List a` first-class, the prelude, module-qualified `List.*` builtins,
//! the list-literal glyph-widening rule, and predicate-based filtering.

mod common;

use common::single_scroll_glyphs;
use emet::{compile, ir::Glyph};

fn main_ty(src: &str) -> String {
    match compile(src) {
        Ok(c) => c.main_ty.to_string(),
        Err(e) => panic!("expected success, got {:?}: {}", e.phase, e.msg),
    }
}

#[test]
fn list_map_builds_glyph_list() {
    let src = r#"
webserver name = aptPackage { name = name }
names = [ "nginx", "openresty" ]
main = [ scroll { name = "test", glyphs = List.map webserver names } ]
"#;
    let rs = single_scroll_glyphs(src);
    assert_eq!(rs.len(), 2);
    assert_eq!(rs[0].key(), "apt:nginx");
    assert_eq!(rs[1].key(), "apt:openresty");
}

#[test]
fn list_concat_map_builds_glyph_list() {
    let src = r#"
pair name = [ aptPackage { name = name }, systemdService { unit = name } ]
main = [ scroll { name = "test", glyphs = List.concatMap pair [ "nginx" ] } ]
"#;
    let rs = single_scroll_glyphs(src);
    assert_eq!(rs.len(), 2);
    assert_eq!(
        rs[0],
        Glyph::AptPackage {
            name: "nginx".into()
        }
    );
    assert_eq!(
        rs[1],
        Glyph::SystemdService {
            unit: "nginx".into()
        }
    );
}

#[test]
fn list_concat_flattens_glyph_lists() {
    let src = r#"
a = [ aptPackage { name = "nginx" } ]
b = [ systemdService { unit = "nginx.service" } ]
main = [ scroll { name = "test", glyphs = List.concat [ a, b ] } ]
"#;
    let rs = single_scroll_glyphs(src);
    assert_eq!(rs.len(), 2);
    assert_eq!(
        rs[0],
        Glyph::AptPackage {
            name: "nginx".into()
        }
    );
    assert_eq!(
        rs[1],
        Glyph::SystemdService {
            unit: "nginx.service".into()
        }
    );
}

#[test]
fn list_append_concatenates_glyph_lists() {
    let src = r#"
main = [ scroll { name = "test", glyphs = List.append [ aptPackage { name = "a" } ] [ aptPackage { name = "b" } ] } ]
"#;
    let rs = single_scroll_glyphs(src);
    assert_eq!(rs.len(), 2);
    assert_eq!(rs[0], Glyph::AptPackage { name: "a".into() });
    assert_eq!(rs[1], Glyph::AptPackage { name: "b".into() });
}

#[test]
fn list_foldr_over_string_list_builds_a_glyph() {
    // foldr (\x acc -> aptPackage { name = x }) is nonsense; instead fold to a
    // string then feed it to a constructor, exercising a fold whose accumulator
    // is a String.
    let src = r#"
first = List.foldr (\x acc -> x) "z" [ "a", "b", "c" ]
main = [ scroll { name = "test", glyphs = [ aptPackage { name = first } ] } ]
"#;
    let rs = single_scroll_glyphs(src);
    assert_eq!(rs, vec![Glyph::AptPackage { name: "a".into() }]);
}

#[test]
fn list_foldl_reaches_last_element() {
    let src = r#"
last = List.foldl (\x acc -> x) "z" [ "a", "b", "c" ]
main = [ scroll { name = "test", glyphs = [ aptPackage { name = last } ] } ]
"#;
    let rs = single_scroll_glyphs(src);
    assert_eq!(rs, vec![Glyph::AptPackage { name: "c".into() }]);
}

#[test]
fn empty_list_is_polymorphic_and_valid_main() {
    let src = r#"
xs : [a]
xs = [ ]
main = [ scroll { name = "test", glyphs = [ ] } ]
"#;
    assert_eq!(main_ty(src), "List Scroll");
    assert!(single_scroll_glyphs(src).is_empty());
}

#[test]
fn homogeneous_string_list_type_checks() {
    let src = r#"
names : List String
names = [ "a", "b" ]
first = List.foldr (\x acc -> x) "z" names
main = [ scroll { name = "test", glyphs = [ aptPackage { name = first } ] } ]
"#;
    assert_eq!(single_scroll_glyphs(src).len(), 1);
}

#[test]
fn bracket_string_type_sugar_parses() {
    let src = r#"
names : [String]
names = [ "a", "b" ]
main = [ scroll { name = "test", glyphs = List.map (\n -> aptPackage { name = n }) names } ]
"#;
    assert_eq!(single_scroll_glyphs(src).len(), 2);
}

#[test]
fn list_filter_keeps_all_on_true_predicate() {
    let src = r#"
main = [ scroll { name = "test", glyphs = List.filter (\x -> True) [ aptPackage { name = "a" }, aptPackage { name = "b" } ] } ]
"#;
    assert_eq!(single_scroll_glyphs(src).len(), 2);
}

#[test]
fn list_filter_drops_all_on_false_predicate() {
    let src = r#"
main = [ scroll { name = "test", glyphs = List.filter (\x -> False) [ aptPackage { name = "a" }, aptPackage { name = "b" } ] } ]
"#;
    assert!(single_scroll_glyphs(src).is_empty());
}

#[test]
fn list_filter_predicate_can_call_is_empty() {
    // `List.isEmpty` feeds a predicate: filtering a list of lists keeps the
    // empty ones, which exercises `isEmpty` yielding `True`/`False` at runtime
    // and `List.filter` inspecting the `Bool` `Data`.
    let src = r#"
kept = List.filter (\xs -> List.isEmpty xs) [ [ ], [ aptPackage { name = "a" } ], [ ] ]
main = [ scroll { name = "test", glyphs = List.concat kept } ]
"#;
    assert!(single_scroll_glyphs(src).is_empty());
}

#[test]
fn is_empty_result_type_is_bool() {
    let src = r#"
flag : Bool
flag = List.isEmpty [ ]
main = [ scroll { name = "test", glyphs = [ ] } ]
"#;
    assert!(single_scroll_glyphs(src).is_empty());
}
