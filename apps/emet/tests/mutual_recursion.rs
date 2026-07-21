mod common;

use emet::ir::Glyph;

fn unit(src: &str) -> String {
    match common::single_scroll_glyphs(src).as_slice() {
        [Glyph::SystemdService { unit }] => unit.clone(),
        other => panic!("expected a single systemdService glyph, got {other:?}"),
    }
}

#[test]
fn is_even_is_odd_two_way_cycle() {
    let src = r#"
isEven n = if n == 0 then "yes" else isOdd (n - 1)
isOdd n = if n == 0 then "no" else isEven (n - 1)
main = [ scroll { name = "test", glyphs = [ systemdService { unit = isEven 10 } ] } ]
"#;
    assert_eq!(unit(src), "yes");
}

#[test]
fn is_odd_calls_is_even_odd_argument() {
    let src = r#"
isEven n = if n == 0 then "yes" else isOdd (n - 1)
isOdd n = if n == 0 then "no" else isEven (n - 1)
main = [ scroll { name = "test", glyphs = [ systemdService { unit = isOdd 7 } ] } ]
"#;
    assert_eq!(unit(src), "yes");
}

#[test]
fn three_way_cycle() {
    let src = r#"
a n = if n == 0 then "a" else b (n - 1)
b n = if n == 0 then "b" else c (n - 1)
c n = if n == 0 then "c" else a (n - 1)
main = [ scroll { name = "test", glyphs = [ systemdService { unit = a 7 } ] } ]
"#;
    assert_eq!(unit(src), "b");
}

#[test]
fn forward_reference_between_non_recursive_decls() {
    let src = r#"
main = [ scroll { name = "test", glyphs = [ systemdService { unit = greeting } ] } ]
greeting = String.append prefix "world"
prefix = "hello "
"#;
    assert_eq!(unit(src), "hello world");
}

#[test]
fn self_recursion_still_works() {
    let src = r#"
fact n = if n == 0 then 1 else n * fact (n - 1)
main = [ scroll { name = "test", glyphs = [ systemdService { unit = String.fromInt (fact 5) } ] } ]
"#;
    assert_eq!(unit(src), "120");
}

#[test]
fn polymorphic_mutually_recursive_pair() {
    let src = r#"
pingList xs = case xs of
  [] -> []
  (x :: rest) -> x :: pongList rest
pongList xs = case xs of
  [] -> []
  (x :: rest) -> x :: pingList rest
usePing = pingList [ aptPackage { name = "a" } ]
useStr = pingList [ "s" ]
main = [ scroll { name = "test", glyphs = usePing } ]
"#;
    let glyphs = common::single_scroll_glyphs(src);
    assert_eq!(glyphs.len(), 1);
    assert_eq!(glyphs[0], Glyph::AptPackage { name: "a".into() });
}

#[test]
fn mutually_recursive_group_with_signature_on_one_member() {
    let src = r#"
isEven : Int -> String
isEven n = if n == 0 then "yes" else isOdd (n - 1)
isOdd n = if n == 0 then "no" else isEven (n - 1)
main = [ scroll { name = "test", glyphs = [ systemdService { unit = isEven 4 } ] } ]
"#;
    assert_eq!(unit(src), "yes");
}
