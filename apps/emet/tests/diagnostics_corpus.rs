//! Promotes the highest-value cases from the diagnostics audit corpus
//! (`apps/emet/tests/corpus/`, tracked in git so a nix flake build sees them;
//! originally graded in the audit's `AUDIT.md`) into a permanent regression
//! suite. Each test reads one corpus program from
//! disk and asserts a *key phrase* of the current `compile()` output — phase
//! and a message substring, not the full rendered text — so the assertions
//! survive incidental wording changes while still pinning the correctness
//! bugs, leak fixes, and new detections the audit's rewrites (Batches K–N)
//! closed. This is the standing gate for AUDIT.md's proposed rewrites: a
//! regression here means a message drifted back toward its pre-audit grade.

use std::path::PathBuf;

fn corpus(name: &str) -> String {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests/corpus");
    p.push(name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

fn err_msg(name: &str) -> (emet::Phase, String) {
    let src = corpus(name);
    match emet::compile(&src) {
        Ok(_) => panic!("{name}: expected a compile error, got success"),
        Err(e) => (e.phase, e.msg),
    }
}

#[test]
fn c08_bad_escape_is_lex_error() {
    let (phase, msg) = err_msg("08-syntax-bad-escape.emet");
    assert_eq!(phase, emet::Phase::Lex, "{msg}");
    assert!(msg.contains("\\q"), "{msg}");
}

#[test]
fn c16_reserved_word_binding() {
    let (phase, msg) = err_msg("16-syntax-keep-as-binding.emet");
    assert_eq!(phase, emet::Phase::Parse, "{msg}");
    assert!(msg.contains("reserved"), "{msg}");
    assert!(!msg.contains("t9"), "leaked typevar: {msg}");
}

#[test]
fn c19_braced_rollback() {
    let (_phase, msg) = err_msg("19-syntax-braced-rollback.emet");
    assert!(msg.contains("without braces"), "{msg}");
}

#[test]
fn c22_angle_bracket_name() {
    let (phase, msg) = err_msg("22-syntax-angle-bracket-name.emet");
    assert_eq!(phase, emet::Phase::Analyze, "{msg}");
    assert!(msg.contains("angle bracket") || msg.contains("<"), "{msg}");
    assert!(msg.contains("name"), "{msg}");
}

#[test]
fn c34_arity_no_typevar_leak() {
    let (_phase, msg) = err_msg("34-type-arity-too-many.emet");
    assert!(!msg.contains("t1"), "leaked typevar: {msg}");
    assert!(!msg.contains("t11"), "leaked typevar: {msg}");
}

#[test]
fn c38_occurs_no_typevar_leak() {
    let (_phase, msg) = err_msg("38-type-occurs.emet");
    assert!(!msg.contains("t1 "), "leaked typevar: {msg}");
}

#[test]
fn c31_number_constraint_plain() {
    let (_phase, msg) = err_msg("31-type-mismatch-int-string.emet");
    assert!(!msg.contains("satisfy"), "jargon: {msg}");
    assert!(msg.to_lowercase().contains("number"), "{msg}");
}

#[test]
fn c46_if_condition_bool() {
    let (_phase, msg) = err_msg("46-type-cond-not-bool.emet");
    assert!(msg.contains("Bool"), "{msg}");
    assert!(msg.to_lowercase().contains("condition"), "{msg}");
}

#[test]
fn c41_policy_field() {
    let (_phase, msg) = err_msg("41-type-policy-given-string.emet");
    assert!(msg.contains("Policy"), "{msg}");
}

#[test]
fn c36_did_you_mean_name() {
    let (_phase, msg) = err_msg("36-type-unbound-nearmiss.emet");
    assert!(msg.contains("greeting"), "{msg}");
    assert!(msg.contains("did you mean"), "{msg}");
}

#[test]
fn c57_did_you_mean_ctor() {
    let (_phase, msg) = err_msg("57-type-unknown-constructor.emet");
    assert!(msg.contains("Nothing"), "{msg}");
}

#[test]
fn c58_did_you_mean_type_ctor() {
    let (_phase, msg) = err_msg("58-type-unknown-type-ctor.emet");
    assert!(msg.contains("String"), "{msg}");
}

#[test]
fn c11_empty_case_no_arms() {
    let (_phase, msg) = err_msg("11-syntax-case-no-arms.emet");
    assert!(
        msg.contains("no arms") || msg.contains("at least one"),
        "{msg}"
    );
}

#[test]
fn c12_arrow_typo_hint() {
    let (phase, msg) = err_msg("12-syntax-arrow-typo.emet");
    assert_eq!(phase, emet::Phase::Parse, "{msg}");
    assert!(msg.contains("=>") && msg.contains("->"), "{msg}");
}

#[test]
fn c21_retry_valid_fields() {
    let (_phase, msg) = err_msg("21-syntax-retry-unknown-field.emet");
    assert!(msg.contains("bogus"), "{msg}");
    assert!(msg.contains("maxAttempts"), "{msg}");
}

#[test]
fn c26_let_in_arm_rejected() {
    let src = corpus("26-syntax-let-in-case-arm.emet");
    let e = emet::compile(&src).unwrap_err();
    assert_ne!(e.span, 0..0, "should locate the arm: {e:?}");
    assert!(
        e.msg.to_lowercase().contains("let") || e.msg.to_lowercase().contains("not yet"),
        "{}",
        e.msg
    );
}

#[test]
fn c50_conflicting_keys_span_and_phase() {
    let src = corpus("50-analyze-conflicting-keys.emet");
    let e = emet::compile(&src).unwrap_err();
    assert_eq!(e.phase, emet::Phase::Analyze, "{}", e.msg);
    assert!(e.msg.contains("/etc/motd"), "{}", e.msg);
    assert_ne!(e.span, 0..0, "should locate a glyph, not the module: {e:?}");
}

#[test]
fn c61_dup_binding_span_and_phase() {
    let src = corpus("61-dup-binding.emet");
    let e = emet::compile(&src).unwrap_err();
    assert_eq!(e.phase, emet::Phase::Parse, "{}", e.msg);
    assert!(e.msg.contains("`x`"), "{}", e.msg);
    assert!(e.msg.contains("twice"), "{}", e.msg);
}
