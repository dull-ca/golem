//! Multi-error parse recovery (ADR 0022): the parser recovers past a bad
//! declaration at a decl/line boundary and keeps parsing, so one compile run
//! reports every independent parse error rather than only the first.

use emet::{compile_all, Phase};

fn parse_errors(src: &str) -> Vec<emet::Error> {
    match compile_all(src) {
        Ok(_) => panic!("expected compile errors, but compilation succeeded"),
        Err(errors) => errors,
    }
}

#[test]
fn two_malformed_decls_report_two_parse_errors() {
    let src = "a = )\nb = )\nmain = [ ]";
    let errors = parse_errors(src);
    let parse: Vec<_> = errors.iter().filter(|e| e.phase == Phase::Parse).collect();
    assert_eq!(
        parse.len(),
        2,
        "expected both malformed decls reported, got {parse:?}"
    );
    assert_ne!(
        parse[0].span, parse[1].span,
        "the two errors must carry distinct spans"
    );
}

#[test]
fn single_malformed_decl_reports_exactly_one_parse_error() {
    let src = "main = )";
    let errors = parse_errors(src);
    let parse: Vec<_> = errors.iter().filter(|e| e.phase == Phase::Parse).collect();
    assert_eq!(
        parse.len(),
        1,
        "one bad decl reports one error, got {parse:?}"
    );
}

#[test]
fn recovery_does_not_cascade_into_a_following_valid_decl() {
    let src = "bad = )\ngood = \"ok\"\nmain = [ ]";
    let errors = parse_errors(src);
    let parse: Vec<_> = errors.iter().filter(|e| e.phase == Phase::Parse).collect();
    assert_eq!(
        parse.len(),
        1,
        "a valid decl after a bad one must not produce a spurious error, got {parse:?}"
    );
}

#[test]
fn a_clean_program_reports_no_errors() {
    let src = "main = [ ]";
    assert!(compile_all(src).is_ok(), "a clean program compiles");
}

#[test]
fn a_parse_clean_program_reports_a_single_type_error() {
    let src = "main = [ nope ]";
    let errors = parse_errors(src);
    assert_eq!(
        errors.len(),
        1,
        "type errors stay first-error, got {errors:?}"
    );
    assert_eq!(errors[0].phase, Phase::Type);
}
