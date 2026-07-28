//! The recursive scroll surface: leaf/branch grouping and optional policy
//! (ADR 0031 §7).

mod common;

use common::err;
use emet::{compile, ir::Contents, ir::Glyph, ir::OnExhaust, ir::Scroll, Phase};

fn scrolls(src: &str) -> Vec<Scroll> {
    match compile(src) {
        Ok(c) => c.scrolls,
        Err(e) => panic!("expected success, got {:?}: {}", e.phase, e.msg),
    }
}

#[test]
fn flat_scroll_still_lowers_to_a_leaf() {
    let src =
        r#"main = [ scroll { name = "db", glyphs = [ aptPackage { name = "postgresql" } ] } ]"#;
    let ss = scrolls(src);
    assert_eq!(ss.len(), 1);
    assert!(ss[0].is_leaf());
    assert_eq!(
        ss[0].glyphs(),
        &[Glyph::AptPackage {
            name: "postgresql".into()
        }]
    );
    assert_eq!(ss[0].policy, None);
}

#[test]
fn groups_build_a_branch_tree_in_source_order() {
    let src = r#"
main =
  [ scroll { name = "worker", groups =
      [ scroll { name = "a", glyphs = [ aptPackage { name = "one" } ] }
      , scroll { name = "b", glyphs = [ aptPackage { name = "two" } ] }
      ] }
  ]
"#;
    let ss = scrolls(src);
    assert_eq!(ss.len(), 1);
    match &ss[0].contents {
        Contents::Groups(children) => {
            assert_eq!(children.len(), 2);
            assert_eq!(children[0].name, "a");
            assert_eq!(children[1].name, "b");
        }
        Contents::Glyphs(_) => panic!("expected groups"),
    }
    let units = ss[0].leaf_units();
    assert_eq!(units[0].path, vec!["worker".to_string(), "a".to_string()]);
}

#[test]
fn scroll_with_both_glyphs_and_groups_is_a_parse_error() {
    let src = r#"main = [ scroll { name = "x", glyphs = [ ], groups = [ ] } ]"#;
    let e = err(src);
    assert_eq!(e.phase, Phase::Parse);
    assert!(
        e.msg.contains("exactly one of `glyphs` or `groups`"),
        "got: {}",
        e.msg
    );
}

#[test]
fn scroll_with_neither_glyphs_nor_groups_is_a_parse_error() {
    let src = r#"main = [ scroll { name = "x" } ]"#;
    let e = err(src);
    assert_eq!(e.phase, Phase::Parse);
    assert!(
        e.msg.contains("exactly one of `glyphs` or `groups`"),
        "got: {}",
        e.msg
    );
}

#[test]
fn keep_policy_lowers_to_on_exhaust_keep() {
    let src = r#"main = [ scroll { name = "x", policy = keep, glyphs = [ aptPackage { name = "one" } ] } ]"#;
    let ss = scrolls(src);
    let policy = ss[0].policy.clone().expect("policy present");
    assert_eq!(policy.on_exhaust, Some(OnExhaust::Keep));
    assert_eq!(policy.max_attempts, None);
}

#[test]
fn rollback_policy_lowers_to_on_exhaust_rollback() {
    let src = r#"main = [ scroll { name = "x", policy = rollback, glyphs = [ aptPackage { name = "one" } ] } ]"#;
    let ss = scrolls(src);
    assert_eq!(
        ss[0].policy.clone().unwrap().on_exhaust,
        Some(OnExhaust::Rollback)
    );
}

#[test]
fn retry_record_sets_the_knobs() {
    let src = r#"
main =
  [ scroll
      { name = "x"
      , policy = retry { maxAttempts = 3, baseDelayMs = 500, backoffMultiplier = 2.0, onExhaust = keep }
      , glyphs = [ aptPackage { name = "one" } ]
      }
  ]
"#;
    let policy = scrolls(src)[0].policy.clone().unwrap();
    assert_eq!(policy.max_attempts, Some(3));
    assert_eq!(policy.base_delay_ms, Some(500));
    assert_eq!(policy.backoff_multiplier, Some(2.0));
    assert_eq!(policy.on_exhaust, Some(OnExhaust::Keep));
    assert_eq!(policy.jitter_fraction, None);
}

#[test]
fn retry_record_sets_the_remaining_knobs() {
    let src = r#"
main =
  [ scroll
      { name = "x"
      , policy = retry { maxDelayMs = 30000, maxElapsedMs = 120000, jitterFraction = 0.25 }
      , glyphs = [ aptPackage { name = "one" } ]
      }
  ]
"#;
    let policy = scrolls(src)[0].policy.clone().unwrap();
    assert_eq!(policy.max_delay_ms, Some(30000));
    assert_eq!(policy.max_elapsed_ms, Some(120000));
    assert_eq!(policy.jitter_fraction, Some(0.25));
    assert_eq!(policy.max_attempts, None);
}

#[test]
fn an_unknown_retry_field_is_a_type_error() {
    let src = r#"main = [ scroll { name = "x", policy = retry { retrys = 3 }, glyphs = [ ] } ]"#;
    let e = err(src);
    assert_eq!(e.phase, Phase::Type);
    assert!(e.msg.contains("retrys"), "got: {}", e.msg);
}

#[test]
fn unknown_retry_field_lists_valid_fields() {
    let src = r#"main = [ scroll { name = "w", glyphs = [], policy = retry { maxAttempts = 3, bogus = 1 } } ]"#;
    let e = match emet::compile(src) {
        Ok(_) => panic!("expected an error"),
        Err(e) => e,
    };
    assert!(e.msg.contains("bogus"), "msg: {}", e.msg);
    assert!(
        e.msg.contains("maxAttempts"),
        "should list valid fields: {}",
        e.msg
    );
}

#[test]
fn groups_must_be_a_scroll_list_not_a_glyph_list() {
    let src = r#"main = [ scroll { name = "x", groups = [ aptPackage { name = "one" } ] } ]"#;
    let e = err(src);
    assert_eq!(e.phase, Phase::Type);
}

#[test]
fn policy_field_must_be_a_policy() {
    let src = r#"main = [ scroll { name = "x", policy = "nope", glyphs = [ ] } ]"#;
    let e = err(src);
    assert_eq!(e.phase, Phase::Type);
}

#[test]
fn retry_backoff_multiplier_must_be_a_float() {
    let src = r#"main = [ scroll { name = "x", policy = retry { backoffMultiplier = "nope" }, glyphs = [ ] } ]"#;
    let e = err(src);
    assert_eq!(e.phase, Phase::Type);
}

#[test]
fn a_library_can_annotate_a_policy_value() {
    let src = r#"
p : Policy
p = retry { maxAttempts = 4 }

main = [ scroll { name = "x", policy = p, glyphs = [ aptPackage { name = "one" } ] } ]
"#;
    let ss = scrolls(src);
    assert_eq!(ss[0].policy.clone().unwrap().max_attempts, Some(4));
}

#[test]
fn same_glyph_key_in_two_sibling_leaves_does_not_conflict() {
    let src = r#"
main =
  [ scroll { name = "host", groups =
      [ scroll { name = "a", glyphs = [ file { path = "/etc/x", contents = "1", mode = "0644" } ] }
      , scroll { name = "b", glyphs = [ file { path = "/etc/x", contents = "2", mode = "0644" } ] }
      ] }
  ]
"#;
    let ss = scrolls(src);
    assert_eq!(ss.len(), 1);
}

#[test]
fn conflicting_keys_within_one_leaf_is_analyze_error() {
    let src = r#"
main =
  [ scroll { name = "host", groups =
      [ scroll { name = "a", glyphs =
          [ file { path = "/etc/x", contents = "1", mode = "0644" }
          , file { path = "/etc/x", contents = "2", mode = "0644" }
          ] }
      ] }
  ]
"#;
    let e = err(src);
    assert_eq!(e.phase, Phase::Analyze);
    assert!(e.msg.contains("file:/etc/x"), "got: {}", e.msg);
}
