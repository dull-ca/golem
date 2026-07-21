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
fn cons_in_expression_prepends() {
    let src = r#"
xs = "a" :: [ "b", "c" ]
first = List.foldr (\x acc -> x) "z" xs
main = [ scroll { name = "test", glyphs = [ systemdService { unit = first } ] } ]
"#;
    assert_eq!(unit(src), "a");
}

#[test]
fn cons_is_right_associative() {
    let src = r#"
xs = "a" :: "b" :: [ ]
n = List.length xs
main = [ scroll { name = "test", glyphs = [ systemdService { unit = String.fromInt n } ] } ]
"#;
    assert_eq!(unit(src), "2");
}

#[test]
fn recursive_map_with_list_patterns() {
    let src = r#"
map f xs =
  case xs of
    [] -> []
    (x :: rest) -> f x :: map f rest
main = [ scroll { name = "test", glyphs = map (\n -> systemdService { unit = n }) [ "a" ] } ]
"#;
    let glyphs = common::single_scroll_glyphs(src);
    assert_eq!(glyphs.len(), 1);
    assert_eq!(glyphs[0], Glyph::SystemdService { unit: "a".into() });
}

#[test]
fn recursive_map_transforms_every_element() {
    let src = r#"
tag n = String.append n ".service"
map f xs =
  case xs of
    [] -> []
    (x :: rest) -> f x :: map f rest
main = [ scroll { name = "test", glyphs = map (\n -> systemdService { unit = tag n }) [ "a", "b", "c" ] } ]
"#;
    let glyphs = common::single_scroll_glyphs(src);
    assert_eq!(glyphs.len(), 3);
    assert_eq!(glyphs[0], Glyph::SystemdService { unit: "a.service".into() });
    assert_eq!(glyphs[2], Glyph::SystemdService { unit: "c.service".into() });
}

#[test]
fn recursive_length_walks_the_list() {
    let src = r#"
length xs =
  case xs of
    [] -> 0
    (x :: rest) -> 1 + length rest
main = [ scroll { name = "test", glyphs = [ systemdService { unit = String.fromInt (length [ "a", "b", "c", "d" ]) } ] } ]
"#;
    assert_eq!(unit(src), "4");
}

#[test]
fn recursive_sum_folds_with_list_patterns() {
    let src = r#"
sum xs =
  case xs of
    [] -> 0
    (x :: rest) -> x + sum rest
main = [ scroll { name = "test", glyphs = [ systemdService { unit = String.fromInt (sum [ 1, 2, 3, 4 ]) } ] } ]
"#;
    assert_eq!(unit(src), "10");
}

#[test]
fn recursive_filter_keeps_matching_elements() {
    let src = r#"
filter p xs =
  case xs of
    [] -> []
    (x :: rest) -> if p x then x :: filter p rest else filter p rest
keep n = n == "yes"
main = [ scroll { name = "test", glyphs = List.map (\n -> systemdService { unit = n }) (filter keep [ "yes", "no", "yes" ]) } ]
"#;
    let glyphs = common::single_scroll_glyphs(src);
    assert_eq!(glyphs.len(), 2);
    assert_eq!(glyphs[0], Glyph::SystemdService { unit: "yes".into() });
    assert_eq!(glyphs[1], Glyph::SystemdService { unit: "yes".into() });
}

#[test]
fn recursive_reverse_with_accumulator() {
    let src = r#"
reverse xs =
  let go acc ys =
        case ys of
          [] -> acc
          (y :: rest) -> go (y :: acc) rest
  in go [] xs
main = [ scroll { name = "test", glyphs = List.map (\n -> systemdService { unit = n }) (reverse [ "a", "b", "c" ]) } ]
"#;
    let glyphs = common::single_scroll_glyphs(src);
    assert_eq!(glyphs.len(), 3);
    assert_eq!(glyphs[0], Glyph::SystemdService { unit: "c".into() });
    assert_eq!(glyphs[2], Glyph::SystemdService { unit: "a".into() });
}

#[test]
fn head_binds_element_and_tail_binds_list() {
    let src = r#"
firstOr d xs =
  case xs of
    [] -> d
    (x :: rest) -> x
main = [ scroll { name = "test", glyphs = [ systemdService { unit = firstOr "empty" [ "head", "b" ] } ] } ]
"#;
    assert_eq!(unit(src), "head");
}

#[test]
fn fixed_length_list_pattern_binds_positionally() {
    let src = r#"
second xs =
  case xs of
    [ a, b ] -> b
    _ -> "other"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = second [ "x", "y" ] } ] } ]
"#;
    assert_eq!(unit(src), "y");
}

#[test]
fn fixed_length_list_pattern_only_matches_exact_length() {
    let src = r#"
second xs =
  case xs of
    [ a, b ] -> b
    _ -> "other"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = second [ "x", "y", "z" ] } ] } ]
"#;
    assert_eq!(unit(src), "other");
}

#[test]
fn nested_list_pattern_destructures_head() {
    let src = r#"
firstOfFirst xss =
  case xss of
    [] -> "empty"
    ((x :: xs) :: rest) -> x
    ([] :: rest) -> "inner-empty"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = firstOfFirst [ [ "deep", "b" ], [ "c" ] ] } ] } ]
"#;
    assert_eq!(unit(src), "deep");
}

#[test]
fn var_arm_covers_the_rest_of_a_list_case() {
    let src = r#"
describe xs =
  case xs of
    [] -> "empty"
    other -> "nonempty"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = describe [ "a" ] } ] } ]
"#;
    assert_eq!(unit(src), "nonempty");
}

#[test]
fn nil_plus_cons_is_exhaustive() {
    let src = r#"
isEmpty xs =
  case xs of
    [] -> "yes"
    (x :: rest) -> "no"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = isEmpty [ ] } ] } ]
"#;
    assert_eq!(unit(src), "yes");
}

#[test]
fn missing_nil_arm_is_non_exhaustive() {
    let src = r#"
firstOf xs =
  case xs of
    (x :: rest) -> x
main = [ scroll { name = "test", glyphs = [ systemdService { unit = firstOf [ "a" ] } ] } ]
"#;
    assert_eq!(err(src).0, Phase::Type);
}

#[test]
fn missing_cons_arm_is_non_exhaustive() {
    let src = r#"
firstOf xs =
  case xs of
    [] -> "empty"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = firstOf [ "a" ] } ] } ]
"#;
    assert_eq!(err(src).0, Phase::Type);
}

#[test]
fn arm_after_catch_all_is_redundant() {
    let src = r#"
firstOf xs =
  case xs of
    other -> "any"
    [] -> "empty"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = firstOf [ "a" ] } ] } ]
"#;
    assert_eq!(err(src).0, Phase::Type);
}

#[test]
fn cons_pattern_binds_head_as_element_type() {
    let src = r#"
sumFirst xs =
  case xs of
    [] -> 0
    (x :: rest) -> x
main = [ scroll { name = "test", glyphs = [ systemdService { unit = String.fromInt (sumFirst [ 7, 8 ]) } ] } ]
"#;
    assert_eq!(unit(src), "7");
}
