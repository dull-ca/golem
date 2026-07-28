//! Tests for the lexer + offside layout algorithm.

use emet::layout::layout_all;
use emet::lexer::{lex, Tok};

fn toks(src: &str) -> Vec<Tok> {
    let lexed = lex(src).expect("lex ok");
    layout_all(lexed).into_iter().map(|t| t.tok).collect()
}

#[test]
fn module_gets_wrapped_in_virtual_braces() {
    let ts = toks("main = x");
    assert_eq!(ts.first(), Some(&Tok::VLBrace));
    assert_eq!(ts.last(), Some(&Tok::Eof));
    // second-to-last should be a closing virtual brace
    assert_eq!(ts[ts.len() - 2], Tok::VRBrace);
}

#[test]
fn two_top_level_decls_separated_by_vsemi() {
    let src = "a = x\nb = y";
    let ts = toks(src);
    let semis = ts.iter().filter(|t| **t == Tok::VSemi).count();
    assert_eq!(
        semis, 1,
        "expected exactly one VSemi between two decls: {ts:?}"
    );
}

#[test]
fn continuation_line_no_semi() {
    // second line is indented further -> continuation, not a new decl
    let src = "main =\n  foo";
    let ts = toks(src);
    let semis = ts.iter().filter(|t| **t == Tok::VSemi).count();
    assert_eq!(
        semis, 0,
        "indented continuation must not insert VSemi: {ts:?}"
    );
}

#[test]
fn let_opens_and_closes_block_multiline() {
    let src = "main =\n  let x = a\n      y = b\n  in x";
    let ts = toks(src);
    // Expect a VLBrace after `let` and a matching VRBrace before `in`.
    let let_pos = ts.iter().position(|t| *t == Tok::Let).unwrap();
    assert_eq!(
        ts[let_pos + 1],
        Tok::VLBrace,
        "let must be followed by VLBrace: {ts:?}"
    );
    let in_pos = ts.iter().position(|t| *t == Tok::In).unwrap();
    assert_eq!(
        ts[in_pos - 1],
        Tok::VRBrace,
        "in must be preceded by VRBrace: {ts:?}"
    );
}

#[test]
fn nested_let_bindings_get_semi() {
    let src = "main =\n  let x = a\n      y = b\n  in x";
    let ts = toks(src);
    // between the two bindings at the same column there should be a VSemi
    let semis = ts.iter().filter(|t| **t == Tok::VSemi).count();
    assert!(
        semis >= 1,
        "expected a VSemi separating let bindings: {ts:?}"
    );
}
