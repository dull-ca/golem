//! The Elm-parity oracle for the `Char`/`String` surface (ADR 0025): one
//! assertion per function, checked against the documented `elm/core` result, so
//! a drift from Elm fails here. Also covers the lexer — every char-literal
//! escape and every rejection (empty/multi-scalar/unterminated/raw-newline/bad
//! `\u`), the `\u{...}` string escape, the `String.fromList ∘ String.toList`
//! round-trip, and an end-to-end image-ref parse (`split`/`slice`/`indexes`).
//!
//! Some expressions use workarounds for gaps elsewhere in the language: there
//! are no hex literals (codepoints written in decimal), negatives are
//! parenthesized (`(-1)`), `lines` is exercised with `\n` only (no `\r`
//! escape to author), and equality is asserted on strings/bools rather than
//! lists, since Emet lists are not `comparable`.

mod common;

use emet::{ir::Glyph, Phase};

fn unit(src: &str) -> String {
    match common::single_scroll_glyphs(src).as_slice() {
        [Glyph::SystemdService { unit }] => unit.clone(),
        other => panic!("expected a single systemdService glyph, got {other:?}"),
    }
}

fn string_of(expr: &str) -> String {
    unit(&common::one_scroll(&format!(
        "[ systemdService {{ unit = {expr} }} ]"
    )))
}

fn int_of(expr: &str) -> String {
    string_of(&format!("String.fromInt ({expr})"))
}

fn bool_of(expr: &str) -> String {
    string_of(&format!("if ({expr}) then \"yes\" else \"no\""))
}

fn char_of(expr: &str) -> String {
    string_of(&format!("String.fromChar ({expr})"))
}

fn strings_of(expr: &str) -> String {
    string_of(&format!("String.join \"|\" ({expr})"))
}

fn ints_of(expr: &str) -> String {
    string_of(&format!(
        "String.join \",\" (List.map String.fromInt ({expr}))"
    ))
}

fn lex_err(expr: &str) -> Phase {
    common::err(&common::one_scroll(&format!(
        "[ systemdService {{ unit = {expr} }} ]"
    )))
    .phase
}

#[test]
fn char_literal_lexes_and_evaluates() {
    assert_eq!(char_of("'a'"), "a");
}

#[test]
fn char_escape_newline() {
    assert_eq!(int_of("Char.toCode '\\n'"), "10");
}

#[test]
fn char_escape_tab() {
    assert_eq!(int_of("Char.toCode '\\t'"), "9");
}

#[test]
fn char_escape_backslash() {
    assert_eq!(int_of("Char.toCode '\\\\'"), "92");
}

#[test]
fn char_escape_single_quote() {
    assert_eq!(int_of("Char.toCode '\\''"), "39");
}

#[test]
fn char_escape_unicode() {
    assert_eq!(int_of("Char.toCode '\\u{1F600}'"), "128512");
}

#[test]
fn char_literal_empty_is_lex_error() {
    assert_eq!(lex_err("''"), Phase::Lex);
}

#[test]
fn char_literal_multi_scalar_is_lex_error() {
    assert_eq!(lex_err("'ab'"), Phase::Lex);
}

#[test]
fn char_literal_unterminated_is_lex_error() {
    assert_eq!(lex_err("'a"), Phase::Lex);
}

#[test]
fn char_literal_raw_newline_is_lex_error() {
    assert_eq!(lex_err("'\n'"), Phase::Lex);
}

#[test]
fn char_literal_bad_unicode_empty_is_lex_error() {
    assert_eq!(lex_err("'\\u{}'"), Phase::Lex);
}

#[test]
fn char_literal_bad_unicode_nonhex_is_lex_error() {
    assert_eq!(lex_err("'\\u{zz}'"), Phase::Lex);
}

#[test]
fn char_literal_bad_unicode_surrogate_is_lex_error() {
    assert_eq!(lex_err("'\\u{D800}'"), Phase::Lex);
}

#[test]
fn unicode_escape_in_string_literal() {
    assert_eq!(int_of("String.length \"\\u{1F600}\""), "1");
    assert_eq!(bool_of("\"\\u{41}\" == \"A\""), "yes");
}

#[test]
fn string_bad_unicode_escape_is_lex_error() {
    assert_eq!(lex_err("\"\\u{D800}\""), Phase::Lex);
}

// Char module, one Elm-parity assertion per function.

#[test]
fn char_to_code() {
    assert_eq!(int_of("Char.toCode 'a'"), "97");
}

#[test]
fn char_from_code() {
    assert_eq!(char_of("Char.fromCode 97"), "a");
}

#[test]
fn char_from_code_out_of_range_is_replacement() {
    assert_eq!(bool_of("Char.fromCode 55296 == '\\u{FFFD}'"), "yes");
    assert_eq!(bool_of("Char.fromCode 1114112 == '\\u{FFFD}'"), "yes");
    assert_eq!(bool_of("Char.fromCode (-1) == '\\u{FFFD}'"), "yes");
}

#[test]
fn char_to_upper() {
    assert_eq!(char_of("Char.toUpper 'a'"), "A");
}

#[test]
fn char_to_lower() {
    assert_eq!(char_of("Char.toLower 'A'"), "a");
}

#[test]
fn char_is_upper() {
    assert_eq!(bool_of("Char.isUpper 'A'"), "yes");
    assert_eq!(bool_of("Char.isUpper 'a'"), "no");
}

#[test]
fn char_is_lower() {
    assert_eq!(bool_of("Char.isLower 'a'"), "yes");
    assert_eq!(bool_of("Char.isLower 'A'"), "no");
}

#[test]
fn char_is_alpha() {
    assert_eq!(bool_of("Char.isAlpha 'a'"), "yes");
    assert_eq!(bool_of("Char.isAlpha '0'"), "no");
}

#[test]
fn char_is_alpha_num() {
    assert_eq!(bool_of("Char.isAlphaNum '9'"), "yes");
    assert_eq!(bool_of("Char.isAlphaNum '-'"), "no");
}

#[test]
fn char_is_digit() {
    assert_eq!(bool_of("Char.isDigit '0'"), "yes");
    assert_eq!(bool_of("Char.isDigit 'a'"), "no");
}

#[test]
fn char_is_oct_digit() {
    assert_eq!(bool_of("Char.isOctDigit '7'"), "yes");
    assert_eq!(bool_of("Char.isOctDigit '8'"), "no");
}

#[test]
fn char_is_hex_digit() {
    assert_eq!(bool_of("Char.isHexDigit 'f'"), "yes");
    assert_eq!(bool_of("Char.isHexDigit 'g'"), "no");
}

#[test]
fn char_is_space() {
    assert_eq!(bool_of("Char.isSpace ' '"), "yes");
    assert_eq!(bool_of("Char.isSpace '\\t'"), "yes");
    assert_eq!(bool_of("Char.isSpace 'x'"), "no");
}

#[test]
fn char_is_comparable() {
    assert_eq!(bool_of("'a' < 'b'"), "yes");
    assert_eq!(bool_of("'a' == 'a'"), "yes");
}

// String module, one Elm-parity assertion per function.

#[test]
fn string_is_empty() {
    assert_eq!(bool_of("String.isEmpty \"\""), "yes");
    assert_eq!(bool_of("String.isEmpty \"x\""), "no");
}

#[test]
fn string_reverse() {
    assert_eq!(string_of("String.reverse \"stressed\""), "desserts");
}

#[test]
fn string_repeat() {
    assert_eq!(string_of("String.repeat 3 \"ha\""), "hahaha");
    assert_eq!(string_of("String.repeat 0 \"ha\""), "");
}

#[test]
fn string_replace() {
    assert_eq!(
        string_of("String.replace \".\" \"-\" \"Json.Decode.succeed\""),
        "Json-Decode-succeed"
    );
}

#[test]
fn string_split() {
    assert_eq!(
        strings_of("String.split \",\" \"cat,dog,cow\""),
        "cat|dog|cow"
    );
}

#[test]
fn string_split_empty_separator() {
    assert_eq!(strings_of("String.split \"\" \"abc\""), "a|b|c");
}

#[test]
fn string_words() {
    assert_eq!(
        strings_of("String.words \"How are \\t you? \\n Good?\""),
        "How|are|you?|Good?"
    );
}

#[test]
fn string_lines() {
    assert_eq!(
        strings_of("String.lines \"How are you?\\nGood?\""),
        "How are you?|Good?"
    );
    assert_eq!(strings_of("String.lines \"a\\nb\\nc\""), "a|b|c");
}

#[test]
fn string_slice_positive() {
    assert_eq!(string_of("String.slice 7 9 \"snakes on a plane!\""), "on");
}

#[test]
fn string_slice_negative() {
    assert_eq!(
        string_of("String.slice (-6) (-1) \"snakes on a plane!\""),
        "plane"
    );
}

#[test]
fn string_left() {
    assert_eq!(string_of("String.left 2 \"Mulder\""), "Mu");
}

#[test]
fn string_right() {
    assert_eq!(string_of("String.right 2 \"Scully\""), "ly");
}

#[test]
fn string_drop_left() {
    assert_eq!(
        string_of("String.dropLeft 2 \"The Lone Gunmen\""),
        "e Lone Gunmen"
    );
}

#[test]
fn string_drop_right() {
    assert_eq!(
        string_of("String.dropRight 2 \"Cigarette Smoking Man\""),
        "Cigarette Smoking M"
    );
}

#[test]
fn string_contains() {
    assert_eq!(bool_of("String.contains \"the\" \"theory\""), "yes");
    assert_eq!(bool_of("String.contains \"z\" \"theory\""), "no");
}

#[test]
fn string_starts_with() {
    assert_eq!(bool_of("String.startsWith \"the\" \"theory\""), "yes");
}

#[test]
fn string_ends_with() {
    assert_eq!(bool_of("String.endsWith \"ory\" \"theory\""), "yes");
}

#[test]
fn string_indexes() {
    assert_eq!(ints_of("String.indexes \"i\" \"Mississippi\""), "1,4,7,10");
}

#[test]
fn string_indexes_empty_needle() {
    assert_eq!(ints_of("String.indexes \"\" \"abc\""), "");
}

#[test]
fn string_indices_alias() {
    assert_eq!(ints_of("String.indices \"i\" \"Mississippi\""), "1,4,7,10");
}

#[test]
fn string_to_list() {
    assert_eq!(string_of("String.fromList (String.toList \"abc\")"), "abc");
    assert_eq!(string_of("String.fromList ['a', 'b', 'c']"), "abc");
}

#[test]
fn string_from_list() {
    assert_eq!(string_of("String.fromList ['a', 'b', 'c']"), "abc");
}

#[test]
fn string_from_char() {
    assert_eq!(string_of("String.fromChar 'x'"), "x");
}

#[test]
fn string_cons() {
    assert_eq!(string_of("String.cons 'T' \"he truth\""), "The truth");
}

#[test]
fn string_to_upper() {
    assert_eq!(string_of("String.toUpper \"skinner\""), "SKINNER");
}

#[test]
fn string_to_lower() {
    assert_eq!(string_of("String.toLower \"X FILES\""), "x files");
}

#[test]
fn string_trim() {
    assert_eq!(string_of("String.trim \"  hats  \""), "hats");
}

#[test]
fn string_trim_left() {
    assert_eq!(string_of("String.trimLeft \"  hats  \""), "hats  ");
}

#[test]
fn string_trim_right() {
    assert_eq!(string_of("String.trimRight \"  hats  \""), "  hats");
}

#[test]
fn string_pad() {
    assert_eq!(string_of("String.pad 5 ' ' \"1\""), "  1  ");
}

#[test]
fn string_pad_left() {
    assert_eq!(string_of("String.padLeft 5 '.' \"1\""), "....1");
}

#[test]
fn string_pad_right() {
    assert_eq!(string_of("String.padRight 5 '.' \"1\""), "1....");
}

#[test]
fn string_map() {
    assert_eq!(
        string_of("String.map (\\c -> if c == '/' then '.' else c) \"a/b/c\""),
        "a.b.c"
    );
}

#[test]
fn string_filter() {
    assert_eq!(string_of("String.filter Char.isDigit \"R2-D2\""), "22");
}

#[test]
fn string_foldl() {
    assert_eq!(string_of("String.foldl String.cons \"\" \"time\""), "emit");
}

#[test]
fn string_foldr() {
    assert_eq!(string_of("String.foldr String.cons \"\" \"time\""), "time");
}

#[test]
fn string_any() {
    assert_eq!(bool_of("String.any Char.isDigit \"90210\""), "yes");
    assert_eq!(bool_of("String.any Char.isDigit \"abc\""), "no");
}

#[test]
fn string_all() {
    assert_eq!(bool_of("String.all Char.isDigit \"90210\""), "yes");
    assert_eq!(bool_of("String.all Char.isDigit \"9021a\""), "no");
}

#[test]
fn string_to_list_round_trips() {
    assert_eq!(
        bool_of("String.fromList (String.toList \"héllo!\") == \"héllo!\""),
        "yes"
    );
}

#[test]
fn image_ref_parse_end_to_end() {
    let src = r#"
ref = "docker.io/library/registry:2"
tagStart = case String.indexes ":" ref of
  (i :: rest) -> i
  [] -> 0
name = String.left tagStart ref
tag = String.dropLeft (tagStart + 1) ref
lastSlash = String.foldl (\c acc -> acc + 1) 0 name
main = [ scroll { name = "test", glyphs = [ systemdService { unit = String.join " " [ name, tag, String.fromInt lastSlash ] } ] } ]
"#;
    assert_eq!(unit(src), "docker.io/library/registry 2 26");
}
