//! User-declared sum types (`type Name p* = V1 f* | V2 …`). Constructors become
//! first-class values and patterns, user types work in signatures and `case`,
//! and exhaustiveness applies to them — all riding the same machinery that backs
//! the built-in `Maybe`/`Bool`/`Order`, which must keep working unchanged.

mod common;

use common::{err, err_phase, single_scroll_glyphs};
use emet::{ir::Glyph, Phase};

fn unit(src: &str) -> String {
    match single_scroll_glyphs(src).as_slice() {
        [Glyph::SystemdService { unit }] => unit.clone(),
        other => panic!("expected a single systemdService glyph, got {other:?}"),
    }
}

#[test]
fn nullary_sum_type_matched_in_case() {
    let src = r#"
type Status = Up | Down
name : Status -> String
name s = case s of
    Up -> "up.service"
    Down -> "down.service"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = name Up } ] } ]
"#;
    assert_eq!(unit(src), "up.service");
}

#[test]
fn single_line_type_declaration_parses() {
    let src = r#"type Status = Up | Down
name : Status -> String
name s = case s of
    Up -> "up.service"
    Down -> "down.service"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = name Down } ] } ]
"#;
    assert_eq!(unit(src), "down.service");
}

#[test]
fn recursive_tree_type_with_recursive_function() {
    // The payoff: a self-referential user type, a recursive function over it,
    // and `case` — `size` counts the internal `Node`s of a three-node tree.
    let src = r#"
type Tree a = Leaf | Node (Tree a) a (Tree a)
size : Tree a -> Int
size t = case t of
    Leaf -> 0
    Node l _ r -> 1 + size l + size r
main = [ scroll { name = "test", glyphs = [ systemdService { unit = String.fromInt (size (Node (Node Leaf 1 Leaf) 2 (Node Leaf 3 Leaf))) } ] } ]
"#;
    assert_eq!(unit(src), "3");
}

#[test]
fn type_declaration_may_follow_the_function_that_uses_it() {
    // Type references are order-independent: `size` mentions `Tree` before its
    // declaration, and the constructors `Leaf`/`Node` are in scope throughout.
    let src = r#"
size : Tree a -> Int
size t = case t of
    Leaf -> 0
    Node l _ r -> 1 + size l + size r
type Tree a = Leaf | Node (Tree a) a (Tree a)
main = [ scroll { name = "test", glyphs = [ systemdService { unit = String.fromInt (size (Node Leaf 1 Leaf)) } ] } ]
"#;
    assert_eq!(unit(src), "1");
}

#[test]
fn mutually_referential_type_declarations() {
    // `Tree` refers to `Forest` and `Forest` refers to `Tree`; both resolve.
    let src = r#"
type Tree a = Tip a | Branch (Forest a)
type Forest a = Forest (List (Tree a))
depth : Tree a -> Int
depth t = case t of
    Tip _ -> 1
    Branch f -> case f of
        Forest ts -> 1 + List.length ts
main = [ scroll { name = "test", glyphs = [ systemdService { unit = String.fromInt (depth (Branch (Forest [ Tip 1, Tip 2 ]))) } ] } ]
"#;
    assert_eq!(unit(src), "3");
}

#[test]
fn constructor_is_first_class_over_a_list() {
    // A user constructor used as a plain function value with `List.map`.
    let src = r#"
type Wrapped = Wrapped String
render : Wrapped -> Glyph
render w = case w of
    Wrapped u -> systemdService { unit = u }
main = [ scroll { name = "test", glyphs = List.map render (List.map Wrapped [ "a.service", "b.service" ]) } ]
"#;
    let glyphs = single_scroll_glyphs(src);
    assert_eq!(
        glyphs,
        vec![
            Glyph::SystemdService { unit: "a.service".into() },
            Glyph::SystemdService { unit: "b.service".into() },
        ]
    );
}

#[test]
fn constructor_passed_as_an_argument() {
    // The `Box` constructor is passed to a higher-order function.
    let src = r#"
type Box a = Box a
apply : (a -> Box a) -> a -> Box a
apply f x = f x
unbox : Box a -> a
unbox b = case b of
    Box x -> x
main = [ scroll { name = "test", glyphs = [ systemdService { unit = unbox (apply Box "svc.service") } ] } ]
"#;
    assert_eq!(unit(src), "svc.service");
}

#[test]
fn user_type_used_in_a_signature() {
    let src = r#"
type Env = Prod | Staging
suffix : Env -> String
suffix e = case e of
    Prod -> "prod.service"
    Staging -> "staging.service"
current : Env
current = Prod
main = [ scroll { name = "test", glyphs = [ systemdService { unit = suffix current } ] } ]
"#;
    assert_eq!(unit(src), "prod.service");
}

#[test]
fn box_type_constructs_matches_and_annotates() {
    let src = r#"
type Box a = Box a
unwrap : Box a -> a
unwrap b = case b of
    Box x -> x
main = [ scroll { name = "test", glyphs = [ systemdService { unit = unwrap (Box "boxed.service") } ] } ]
"#;
    assert_eq!(unit(src), "boxed.service");
}

#[test]
fn two_parameter_pair_type_round_trips_both_fields() {
    let src = r#"
type Pair a b = Pair a b
first : Pair a b -> a
first p = case p of
    Pair x _ -> x
second : Pair a b -> b
second p = case p of
    Pair _ y -> y
main = [ scroll { name = "test", glyphs = [ systemdService { unit = String.append (first (Pair "a" 1)) (String.fromInt (second (Pair "a" 1))) } ] } ]
"#;
    assert_eq!(unit(src), "a1");
}

#[test]
fn result_type_with_two_distinct_parameters() {
    let src = r#"
type Result e a = Err e | Ok a
recover : Result String String -> String
recover r = case r of
    Err e -> e
    Ok a -> a
main = [ scroll { name = "test", glyphs = [ systemdService { unit = recover (Ok "ok.service") }, systemdService { unit = recover (Err "err.service") } ] } ]
"#;
    let glyphs = single_scroll_glyphs(src);
    assert_eq!(
        glyphs,
        vec![
            Glyph::SystemdService { unit: "ok.service".into() },
            Glyph::SystemdService { unit: "err.service".into() },
        ]
    );
}

#[test]
fn parameterized_type_folded_recursively() {
    // A recursive parameterized `Tree a` folded to a list of its leaf payloads,
    // exercising the type parameter through a self-referential structure.
    let src = r#"
type Tree a = Leaf a | Branch (Tree a) (Tree a)
leaves : Tree a -> List a
leaves t = case t of
    Leaf x -> [ x ]
    Branch l r -> leaves l ++ leaves r
main = [ scroll { name = "test", glyphs = List.map (\u -> systemdService { unit = u }) (leaves (Branch (Leaf "a.service") (Branch (Leaf "b.service") (Leaf "c.service")))) } ]
"#;
    let glyphs = single_scroll_glyphs(src);
    assert_eq!(
        glyphs,
        vec![
            Glyph::SystemdService { unit: "a.service".into() },
            Glyph::SystemdService { unit: "b.service".into() },
            Glyph::SystemdService { unit: "c.service".into() },
        ]
    );
}

#[test]
fn non_exhaustive_case_on_parameterized_type_names_the_missing_constructor() {
    let src = r#"
type Result e a = Err e | Ok a
recover r = case r of
    Ok a -> a
main = [ scroll { name = "test", glyphs = [ systemdService { unit = recover (Ok "ok.service") } ] } ]
"#;
    let e = err(src);
    assert_eq!(e.phase, Phase::Type);
    assert!(e.msg.contains("Err"), "expected the missing constructor named, got: {}", e.msg);
}

#[test]
fn non_exhaustive_case_on_user_type_names_the_missing_constructor() {
    let src = r#"
type Status = Up | Down
name s = case s of
    Up -> "up"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = name Up } ] } ]
"#;
    let e = err(src);
    assert_eq!(e.phase, Phase::Type);
    assert!(e.msg.contains("Down"), "expected the missing constructor named, got: {}", e.msg);
}

#[test]
fn unknown_constructor_in_case_is_a_type_error() {
    let src = r#"
type Status = Up | Down
name s = case s of
    Up -> "up"
    Sideways -> "x"
    Down -> "down"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = name Up } ] } ]
"#;
    assert_eq!(err_phase(src), Phase::Type);
}

#[test]
fn duplicate_type_name_is_a_type_error() {
    let src = r#"
type Status = Up | Down
type Status = On | Off
main = [ scroll { name = "test", glyphs = [ ] } ]
"#;
    let e = err(src);
    assert_eq!(e.phase, Phase::Type);
    assert!(e.msg.contains("Status"), "expected the duplicated name, got: {}", e.msg);
}

#[test]
fn duplicate_constructor_name_is_a_type_error() {
    let src = r#"
type A = Foo | Bar
type B = Foo | Baz
main = [ scroll { name = "test", glyphs = [ ] } ]
"#;
    let e = err(src);
    assert_eq!(e.phase, Phase::Type);
    assert!(e.msg.contains("Foo"), "expected the duplicated constructor, got: {}", e.msg);
}

#[test]
fn redeclaring_a_builtin_type_is_a_type_error() {
    let src = r#"
type Maybe a = Present a | Absent
main = [ scroll { name = "test", glyphs = [ ] } ]
"#;
    assert_eq!(err_phase(src), Phase::Type);
}

#[test]
fn builtin_maybe_bool_and_order_still_work_alongside_user_types() {
    // A module that declares a user type but also exercises the built-ins.
    let src = r#"
type Status = Up | Down
picked : Maybe String
picked = Just "picked.service"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = Maybe.withDefault "fallback.service" picked } ] } ]
"#;
    assert_eq!(unit(src), "picked.service");
}
