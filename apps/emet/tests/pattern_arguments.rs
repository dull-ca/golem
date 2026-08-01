mod common;
use common::err;
use emet::{compile, Phase};

#[test]
fn declaration_parameter_destructures_a_single_constructor() {
    let src = r#"
type Box = Box { label : String }

labelOf (Box spec) = spec.label

main = [ scroll { name = labelOf (Box { label = "b" }), glyphs = [] } ]
"#;
    let c = compile(src).expect("a single-constructor parameter pattern should compile");
    assert_eq!(c.scrolls[0].name, "b");
}

#[test]
fn destructured_parameter_mixes_with_plain_parameters() {
    let src = r#"
type Box = Box { label : String }

joined prefix (Box spec) suffix = prefix ++ spec.label ++ suffix

main = [ scroll { name = joined "a" (Box { label = "b" }) "c", glyphs = [] } ]
"#;
    let c = compile(src).expect("a destructured parameter should curry like any other");
    assert_eq!(c.scrolls[0].name, "abc");
}

#[test]
fn lambda_parameter_destructures_a_single_constructor() {
    let src = r#"
type Box = Box { label : String }

main = [ scroll { name = (\(Box spec) -> spec.label) (Box { label = "b" }), glyphs = [] } ]
"#;
    let c = compile(src).expect("a single-constructor lambda pattern should compile");
    assert_eq!(c.scrolls[0].name, "b");
}

#[test]
fn destructured_parameter_infers_its_type_without_a_signature() {
    let src = r#"
type Wrapped = Wrapped String

unwrap : Wrapped -> String
unwrap (Wrapped inner) = inner

main = [ scroll { name = unwrap (Wrapped "b"), glyphs = [] } ]
"#;
    let c = compile(src).expect("a destructured parameter should check against its signature");
    assert_eq!(c.scrolls[0].name, "b");
}

#[test]
fn multi_constructor_type_is_rejected_and_points_at_case() {
    let src = r#"
type Result e a = Err e | Ok a

unwrap (Ok value) = value

main = [ scroll { name = unwrap (Ok "b"), glyphs = [] } ]
"#;
    let e = err(src);
    assert_eq!(e.phase, Phase::Type);
    assert!(
        e.msg.contains("`Ok`") && e.msg.contains("`Result`"),
        "the message must name the constructor and its type, got: {}",
        e.msg
    );
    assert!(
        e.msg.contains("case") || e.note.as_deref().unwrap_or("").contains("case"),
        "the message must direct the author to `case`, got: {} / {:?}",
        e.msg,
        e.note
    );
}

#[test]
fn multi_constructor_lambda_pattern_is_rejected() {
    let src = r#"
main = [ scroll { name = (\(Just x) -> x) (Just "b"), glyphs = [] } ]
"#;
    let e = err(src);
    assert_eq!(e.phase, Phase::Type);
    assert!(
        e.msg.contains("`Maybe`"),
        "the message must name the type, got: {}",
        e.msg
    );
}

#[test]
fn a_glyph_tag_may_not_be_destructured_in_an_argument() {
    let src = r#"
pathOf (LineInFile path line) = path

main = [ scroll { name = pathOf (lineInFile { path = "/p", line = "l" }), glyphs = [] } ]
"#;
    let e = err(src);
    assert_eq!(e.phase, Phase::Type);
    assert!(
        e.msg.contains("`Glyph`"),
        "the message must name the sum type, got: {}",
        e.msg
    );
}

#[test]
fn arity_mismatch_in_a_parameter_pattern_is_an_error() {
    let src = r#"
type Pair = Pair String String

first (Pair a) = a

main = [ scroll { name = first (Pair "b" "c"), glyphs = [] } ]
"#;
    let e = err(src);
    assert_eq!(e.phase, Phase::Type);
    assert!(
        e.msg.contains("`Pair`") && e.msg.contains("2"),
        "the message must name the constructor and its arity, got: {}",
        e.msg
    );
}

#[test]
fn unknown_constructor_in_a_parameter_pattern_is_an_error() {
    let src = r#"
type Box = Box String

unwrap (Bx inner) = inner

main = [ scroll { name = unwrap (Box "b"), glyphs = [] } ]
"#;
    let e = err(src);
    assert_eq!(e.phase, Phase::Type);
    assert!(
        e.msg.contains("unknown constructor") && e.msg.contains("`Box`"),
        "the message must reject the name and suggest the real one, got: {}",
        e.msg
    );
}

#[test]
fn the_multi_constructor_message_reads_as_written() {
    let src = r#"
unwrap (Just x) = x

main = [ scroll { name = unwrap (Just "b"), glyphs = [] } ]
"#;
    let e = err(src);
    assert_eq!(
        e.msg,
        "`Just` is one of several constructors of `Maybe`, so this pattern could fail"
    );
    assert_eq!(
        e.note.as_deref(),
        Some(
            "an argument pattern must always match. Take the whole `Maybe` as a parameter and branch on it with `case … of`, which is checked for exhaustiveness."
        )
    );
}
