# Emet Friendly Diagnostics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Raise the Emet compiler's error messages toward Elm's "compiler errors for humans" bar — friendly typevar names, clean expected-sets, plain-language constraint messages, did-you-mean suggestions, and new detections for silent-accept correctness bugs — as catalogued in `.superpowers/sdd/errmsg/AUDIT.md`.

**Architecture:** Cross-cutting renderers land first (friendly type-variable display, filename-label strip, cleaned chumsky expected-sets). Then new detections are added stage by stage: lexer (bad string escape), parser (reserved-word binding, syntax hints, duplicate `let` bindings, angle-bracket names), inference (did-you-mean, plain constraint wording, arity/occurs, reversed framing, duplicate top-level bindings), then span-threading from eval into analyze/main-type. A final task promotes selected audit corpus cases into a permanent regression suite.

**Tech Stack:** Rust, `chumsky` 0.10 (parser, `Rich` errors, `.validate` non-fatal emit), `ariadne` (rendering, in `main.rs`). Compiler crate `apps/emet/`. Tests via `cargo test -p emet`.

## Global Constraints

- **Zero comments in implementation code.** The documentation agent owns all comments; every task carries a `Doc backlog:` line naming what a documenter should later annotate. Do not write `//` or `///` comments in the code you add.
- **TDD red-green.** Every new detection and every reworded message lands with a failing test written first, run to confirm it fails, then the minimal implementation, then a green run.
- **Out of scope (do not attempt):** general parse-error recovery / one-clean-error resynchronisation. Deferred by `docs/adr/0032-friendly-elm-style-diagnostics.md` (being written in parallel). Reference that ADR where a cascade is left in place; do not build recovery machinery.
- **Small dependency footprint:** `ariadne` + `chumsky` only. Do not add crates (edit-distance is a ~30-line local function, not a dependency).
- **Test commands:** `cargo test -p emet` for the whole crate; `cargo test -p emet --test <file>` for one file (e.g. `--test diagnostics`).
- **Git:** never `git push`. Stage only the files you changed with `git add <paths>` (never `git add -A`/`git add .`). Every commit ends with the trailer line:
  `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`
- **Message-text target:** where the AUDIT's per-finding "Rewrite:" gives an exact string, that rewrite text is the target. Tests assert the load-bearing *key phrase*, not the whole sentence, so implementations may vary punctuation but must contain the asserted phrase.

---

## Task 1: Friendly type-variable rendering (`render_type`)

Replace the leaked `t9`, `t11 -> t12`, `{ } -> t2` internal variable names (#16, #19, #34, #38) with Elm-style `a`, `b`, `c`. `Type`'s `Display` cannot carry the first-seen-order map, so add a free function `render_type` that walks a `Type`, numbers each distinct `Var(n, _)` in first-seen order, and prints it as a lowercase letter. Route every message site that currently interpolates an applied `Type` through it.

**Files:**
- Modify: `apps/emet/src/infer.rs` (add `render_type`; change the `bind`/`unify`/`main`-type/arity message sites)
- Test: `apps/emet/tests/diagnostics.rs`

**Interfaces:**
- Produces: `pub(crate) fn render_type(t: &crate::ast::Type) -> String` in `infer.rs` — renders a type with unification vars as `a`, `b`, `c`, …, `z`, `a1`, `b1`, … in first-seen order; concrete parts render exactly as `Type`'s `Display` does. Later tasks (arity #34, occurs #38) call it.

- [ ] **Step 1: Write the failing test**

Add to `apps/emet/tests/diagnostics.rs`:

```rust
#[test]
fn arity_too_many_does_not_leak_internal_typevars() {
    let e = err("f : Int -> Int\nf x = x\nmain = [ f 1 2 ]");
    assert_eq!(e.phase, Phase::Type);
    assert!(!e.msg.contains("t1"), "leaked internal typevar: {}", e.msg);
    assert!(!e.msg.contains("t2"), "leaked internal typevar: {}", e.msg);
    assert!(!e.msg.contains("t9"), "leaked internal typevar: {}", e.msg);
}

#[test]
fn occurs_error_renders_friendly_typevars() {
    let e = err("main = (\\x -> x x)");
    assert_eq!(e.phase, Phase::Type);
    assert!(!e.msg.contains("t1"), "leaked internal typevar: {}", e.msg);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p emet --test diagnostics arity_too_many_does_not_leak_internal_typevars occurs_error_renders_friendly_typevars`
Expected: FAIL — messages contain `t9`/`t1` (leaked).

- [ ] **Step 3: Add `render_type`**

Add to `apps/emet/src/infer.rs` (top-level function, near `constraint_name`):

```rust
pub(crate) fn render_type(t: &Type) -> String {
    let mut names: HashMap<u32, String> = HashMap::new();
    let mut next: u32 = 0;
    let mut out = String::new();
    render_into(t, &mut names, &mut next, &mut out, false);
    out
}

fn var_letter(n: u32) -> String {
    let letter = (b'a' + (n % 26) as u8) as char;
    let cycle = n / 26;
    if cycle == 0 {
        letter.to_string()
    } else {
        format!("{letter}{cycle}")
    }
}

fn render_into(t: &Type, names: &mut HashMap<u32, String>, next: &mut u32, out: &mut String, paren_fun: bool) {
    match t {
        Type::Var(n, _) => {
            let name = names.entry(*n).or_insert_with(|| {
                let s = var_letter(*next);
                *next += 1;
                s
            });
            out.push_str(name);
        }
        Type::Rigid(name) => out.push_str(name),
        Type::Con(name, args) if args.is_empty() => out.push_str(name),
        Type::Con(name, args) => {
            out.push_str(name);
            for arg in args {
                out.push(' ');
                let wrap = matches!(arg, Type::Con(_, inner) if !inner.is_empty())
                    || matches!(arg, Type::Fun(_, _));
                if wrap {
                    out.push('(');
                    render_into(arg, names, next, out, false);
                    out.push(')');
                } else {
                    render_into(arg, names, next, out, false);
                }
            }
        }
        Type::Fun(a, b) => {
            let wrap_a = matches!(**a, Type::Fun(_, _));
            if paren_fun {
                out.push('(');
            }
            if wrap_a {
                out.push('(');
                render_into(a, names, next, out, false);
                out.push(')');
            } else {
                render_into(a, names, next, out, false);
            }
            out.push_str(" -> ");
            render_into(b, names, next, out, false);
            if paren_fun {
                out.push(')');
            }
        }
        Type::Record(fields, row) => {
            out.push_str("{ ");
            for (i, (k, v)) in fields.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(k);
                out.push_str(" : ");
                render_into(v, names, next, out, false);
            }
            if let Row::Open(_) = row {
                out.push_str(" | ..");
            }
            out.push_str(" }");
        }
        Type::Tuple(elems) => {
            out.push('(');
            for (i, elem) in elems.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                render_into(elem, names, next, out, false);
            }
            out.push(')');
        }
    }
}
```

- [ ] **Step 4: Route the four leak sites through it**

In `apps/emet/src/infer.rs`, change these interpolations from `self.apply(&…)`/inline `t{v}` to `render_type(&self.apply(&…))`:

`bind` occurs-check message (was line ~366):
```rust
        if self.occurs(v, &t) {
            return Err(TypeError::new(
                format!("infinite type: `{}` occurs in itself", render_type(&self.apply(&t))),
                span.clone(),
            ));
        }
```

`bind` constraint message (was line ~372) — leave the constraint wording to Task 8, but swap the type render now:
```rust
        if !constraint_admits(c, &t) {
            return Err(TypeError::new(
                format!(
                    "type `{}` does not satisfy `{}`",
                    render_type(&self.apply(&t)),
                    constraint_name(c)
                ),
                span.clone(),
            ));
        }
```

`unify` Con-mismatch (was line ~395) and the catch-all mismatch (was line ~426): both format `expected …, found …` — replace `self.apply(&a)` / `self.apply(&b)` with `render_type(&self.apply(&a))` / `render_type(&self.apply(&b))`.

`finish_main` (was line ~2312): replace `{main_ty}` with `render_type(&main_ty)`:
```rust
        None => Err(TypeError::new(
            format!("`main` must be `List Scroll` (a list of scrolls), but is `{}`", render_type(&main_ty)),
            0..0,
        )),
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p emet --test diagnostics`
Expected: PASS (both new tests green; existing `type mismatch` / `List Scroll` / `unknown type constructor` assertions still hold — those substrings are unchanged).

- [ ] **Step 6: Run the whole crate to catch collateral**

Run: `cargo test -p emet`
Expected: PASS. If any test asserts a concrete-type render substring (e.g. `String -> SystemdService`), confirm `render_type` prints concrete types identically — it does, since the only visible change is `t{n}` → letters. No locked assertion in the catalog pins a `t{n}` string.

- [ ] **Step 7: Commit**

```bash
git add apps/emet/src/infer.rs apps/emet/tests/diagnostics.rs
git commit -m "feat(emet): render type variables as friendly names in diagnostics

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** document `render_type` / `render_into` (why Display can't hold the first-seen map; the `a`,`b`,`c` scheme; that concrete parts match `Display`).

**Locked-assertion updates:** none. This task changes no asserted substring; it only removes `t{n}` leaks, which no test pins.

---

## Task 2: Strip the filename prefix from parse-error labels

Every diagnostic from a file build reads `…/NN-foo.emet: found '}' expected …`. The path is already in the ariadne header. `resolve.rs:242` prepends `"{path}: "` to every recovered file error. Remove the prepend (the AUDIT's item 1) so the label carries only the message.

**Files:**
- Modify: `apps/emet/src/resolve.rs:240-245`
- Test: `apps/emet/tests/library_search_path.rs` (build-path errors are observable there)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: file-build errors whose `msg` no longer begins with `"{path}: "`.

- [ ] **Step 1: Write the failing test**

Add to `apps/emet/tests/library_search_path.rs` (top uses `emet::compile_file`; mirror its existing fixture helper — inspect the file's `write_project`/`tempdir` helper and reuse it). Write a bad-parse entry and assert no path prefix:

```rust
#[test]
fn parse_error_msg_has_no_filename_prefix() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("Main.emet");
    std::fs::write(&entry, "main = [ \"a\" ").unwrap();
    let errors = emet::compile_file_all(&entry).unwrap_err();
    let e = &errors[0];
    assert!(!e.msg.contains(".emet:"), "filename leaked into label: {}", e.msg);
    assert!(!e.msg.contains(entry.to_str().unwrap()), "path leaked: {}", e.msg);
}
```

If `tempfile` is not already a dev-dependency here, use the file's existing project-fixture helper instead of `tempfile::tempdir` — check the top of `library_search_path.rs` for the established pattern and match it exactly.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p emet --test library_search_path parse_error_msg_has_no_filename_prefix`
Expected: FAIL — `msg` contains `.emet:`.

- [ ] **Step 3: Remove the prepend**

In `apps/emet/src/resolve.rs`, change:

```rust
    let module = crate::parse_source_multi(&source).map_err(|mut errors| {
        for e in &mut errors {
            e.msg = format!("{}: {}", path.display(), e.msg);
        }
        errors
    })?;
```

to:

```rust
    let module = crate::parse_source_multi(&source)?;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p emet --test library_search_path`
Expected: PASS. The `cannot find imported module` and `cycle` assertions (lines 149, 109) are unaffected — those messages come from `missing_module_message`/cycle detection, not the stripped prepend.

- [ ] **Step 5: Run the whole crate**

Run: `cargo test -p emet`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/emet/src/resolve.rs apps/emet/tests/library_search_path.rs
git commit -m "fix(emet): drop the filename prefix from file-build error labels

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** note (in `resolve.rs`) that the path is now carried only by the ariadne header, not the message.

**Locked-assertion updates:** none removed. `library_search_path.rs` keeps `cannot find imported module` (149) and `cycle` (109). One assertion *added* (this task's test).

---

## Task 3: Clean the chumsky expected-set (`humanize_expected`)

Parse errors end in raw chumsky `expected …` lists: `expected something else, '(', '[', or '{'`, `expected ';', 'type', something else, '}', '}', or end of input` (#04, #05, #06, #12, #23, #24, #25, #29). Post-process the `Rich` string in `parser::parse` so the user-facing message: drops virtual tokens (`;`, and the `}` that came from `VRBrace`), dedupes, and replaces `something else` with `an expression`.

**Files:**
- Modify: `apps/emet/src/parser.rs` (`parse`, add `humanize_expected`)
- Test: `apps/emet/tests/diagnostics.rs`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `fn humanize_expected(raw: &str) -> String` in `parser.rs` — takes chumsky's `Rich::to_string()` output and returns a cleaned string with virtual tokens removed, duplicates collapsed, and `something else` → `an expression`. Used by `parse`.

**Note on chumsky's format:** `Rich::to_string()` renders as `found '<tok>' expected <a>, <b>, or <c>` (or `expected something else` / `expected end of input`). Because `Tok`'s `Display` prints `VSemi` as `;` and `VRBrace` as `}` (identical to real `RBrace`), a virtual close is indistinguishable from a real one in the string. The cleaner therefore operates on the rendered token spellings: it removes any `';'` entry (never user-typed at expression start), collapses duplicate `'}'` entries to one, and rewrites `something else`.

- [ ] **Step 1: Write the failing test**

Add to `apps/emet/tests/diagnostics.rs`:

```rust
#[test]
fn expected_set_has_no_virtual_semicolon() {
    let e = err("main =\n  let x = 1\n  x");
    assert_eq!(e.phase, Phase::Parse);
    assert!(!e.msg.contains("';'"), "virtual ; leaked: {}", e.msg);
}

#[test]
fn expected_set_has_no_something_else() {
    let e = err("f x x + 1\n\nmain = f 2");
    assert_eq!(e.phase, Phase::Parse);
    assert!(!e.msg.contains("something else"), "jargon leaked: {}", e.msg);
    assert!(e.msg.contains("an expression"), "expected replacement: {}", e.msg);
}

#[test]
fn expected_set_dedupes_repeated_brace() {
    let e = err("main = x + * y");
    assert_eq!(e.phase, Phase::Parse);
    let n = e.msg.matches("'}'").count();
    assert!(n <= 1, "duplicate '}}' in: {}", e.msg);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p emet --test diagnostics expected_set_`
Expected: FAIL — raw messages leak `';'`, `something else`, and duplicate `'}'`.

- [ ] **Step 3: Add `humanize_expected` and call it**

Add to `apps/emet/src/parser.rs`:

```rust
fn humanize_expected(raw: &str) -> String {
    let Some(idx) = raw.find("expected ") else {
        return raw.replace("something else", "an expression");
    };
    let (head, tail) = raw.split_at(idx);
    let list = &tail["expected ".len()..];
    let mut items: Vec<String> = Vec::new();
    for piece in list.split(", ") {
        let piece = piece.strip_prefix("or ").unwrap_or(piece);
        let cleaned = piece.trim().replace("something else", "an expression");
        if cleaned == "';'" {
            continue;
        }
        if items.iter().any(|existing| existing == &cleaned) {
            continue;
        }
        items.push(cleaned);
    }
    if items.is_empty() {
        return format!("{head}expected an expression");
    }
    let joined = match items.len() {
        1 => items[0].clone(),
        2 => format!("{} or {}", items[0], items[1]),
        _ => {
            let last = items.pop().unwrap();
            format!("{}, or {}", items.join(", "), last)
        }
    };
    format!("{head}expected {joined}")
}
```

In `parse`, change the error mapping (was line ~1233):

```rust
        let out: Vec<ParseError> = errors
            .into_iter()
            .map(|e| ParseError {
                msg: humanize_expected(&e.to_string()),
                span: span_range(*e.span()),
            })
            .collect();
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p emet --test diagnostics`
Expected: PASS. `leading_junk_reports_the_offending_token` still holds (`')'` and `expected` survive), and `bad_type_after_colon` still finds `a type` (a labelled set, not `something else`).

- [ ] **Step 5: Run the whole crate**

Run: `cargo test -p emet`
Expected: PASS. Custom-message errors (`Rich::custom` from `build_constructor`, `fold_operators`, tuple/float rejects) have no `expected ` clause, so `humanize_expected` returns them via the early `replace` path unchanged.

- [ ] **Step 6: Commit**

```bash
git add apps/emet/src/parser.rs apps/emet/tests/diagnostics.rs
git commit -m "feat(emet): clean the parser expected-set (dedupe, drop virtual tokens, plain wording)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** document `humanize_expected` (why post-processing the rendered string rather than inspecting `Rich`; that `VSemi`/`VRBrace` are indistinguishable from real tokens by spelling; the `something else` → `an expression` mapping).

**Locked-assertion updates:** none broken. `diagnostics.rs` `expected`/`a type`/`')'`/`']'` assertions all survive. Three assertions added.

---

## Task 4: Point unclosed-delimiter errors at the opener (#01/#02/#03)

The unclosed `{` / `[` / `(` errors underline the *following* token (`}`, `]`, or a virtual `;`) rather than the opener, and say `found '}' expected ',' or '}'`. This is a span + wording change. Because full recovery is out of scope, target the achievable win: when the found token is a closing/virtual token and the expected set includes a matching close, keep chumsky's location but improve the message. The audit's ideal (underline the opener) needs delimiter-span tracking the current grammar does not thread; implement the *wording* half here and record the span half as deferred to ADR 0032.

**Files:**
- Modify: `apps/emet/src/parser.rs` (`humanize_expected` or a sibling pass in `parse`)
- Test: `apps/emet/tests/diagnostics.rs`

**Interfaces:**
- Consumes: `humanize_expected` (Task 3).
- Produces: the same `parse` error-mapping, now also rewriting a bare `found ',' or '}'`-style close-only expectation into a `looks unclosed` hint.

**Scope honesty:** underlining the opener requires the parser to carry each open-delimiter's span to its close-failure — the current chumsky grammar does not. That span move is a recovery-adjacent change deferred to `docs/adr/0032-friendly-elm-style-diagnostics.md`. This task delivers only the message improvement at chumsky's existing span.

- [ ] **Step 1: Write the failing test**

Add to `apps/emet/tests/diagnostics.rs`:

```rust
#[test]
fn unclosed_bracket_message_mentions_unclosed() {
    let e = err(r#"main = [ "a" "#);
    assert_eq!(e.phase, Phase::Parse);
    assert!(
        e.msg.to_lowercase().contains("close") || e.msg.to_lowercase().contains("unclosed"),
        "expected an unclosed hint: {}",
        e.msg
    );
    assert!(e.msg.contains("']'"), "should still name the delimiter: {}", e.msg);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p emet --test diagnostics unclosed_bracket_message_mentions_unclosed`
Expected: FAIL — message is `found ';' expected ',' or ']'` with no "close"/"unclosed".

- [ ] **Step 3: Add the close-only hint to the humanizer**

In `apps/emet/src/parser.rs`, extend `humanize_expected` so that after building `items`, when every remaining item is a closing delimiter or a comma, it appends a hint. Insert before the final `format!` return:

```rust
    let only_closers = items
        .iter()
        .all(|i| matches!(i.as_str(), "','" | "')'" | "']'" | "'}'" | "`}`"));
    let close_hint = if only_closers {
        items
            .iter()
            .find(|i| matches!(i.as_str(), "')'" | "']'" | "'}'" | "`}`"))
            .map(|d| format!(" — this looks like an unclosed {d}"))
            .unwrap_or_default()
    } else {
        String::new()
    };
```

and change the final return to append it:

```rust
    format!("{head}expected {joined}{close_hint}")
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p emet --test diagnostics unclosed_bracket`
Expected: PASS — both `unclosed_bracket_reports_closing_bracket` (still finds `']'`, span `13..13`) and the new hint test are green.

- [ ] **Step 5: Run the whole crate**

Run: `cargo test -p emet`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/emet/src/parser.rs apps/emet/tests/diagnostics.rs
git commit -m "feat(emet): hint 'unclosed delimiter' on close-only parse expectations

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** note that opener-span underlining is deferred to ADR 0032; this only improves wording at chumsky's span.

**Locked-assertion updates:** `diagnostics.rs::unclosed_bracket_reports_closing_bracket` (line 28) still asserts `']'` and span `13..13` — unchanged, verified green. One assertion added.

---

## Task 5: Bad string escape becomes a lex error (#08)

`"bad \q escape"` silently drops `\q` and surfaces as a `main` type error. The char-literal path already errors ("invalid char literal escape") on an unknown escape; the string path (`scan_string_segment`) maps `\x` → `x` for anything unrecognized. Reject unknown string escapes at the lexer.

**Files:**
- Modify: `apps/emet/src/lexer.rs` (`scan_string_segment`, the `other => other` arm)
- Test: `apps/emet/tests/char_and_string.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: a `LexError` for unknown string escapes; valid escapes (`\n \t \\ \" \${ \u{...}`) unchanged.

- [ ] **Step 1: Write the failing test**

Add to `apps/emet/tests/char_and_string.rs`:

```rust
#[test]
fn unknown_string_escape_is_a_lex_error() {
    let e = match emet::compile(r#"main = "bad \q escape""#) {
        Ok(_) => panic!("expected an error"),
        Err(e) => e,
    };
    assert_eq!(e.phase, emet::Phase::Lex);
    assert!(e.msg.contains("\\q"), "should name the bad escape: {}", e.msg);
}

#[test]
fn valid_string_escapes_still_lex() {
    let c = emet::compile("main = [ ]\nmsg = \"a\\nb\\t\\\\\\\"c\"");
    assert!(c.is_ok(), "valid escapes should lex: {c:?}");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p emet --test char_and_string unknown_string_escape_is_a_lex_error valid_string_escapes_still_lex`
Expected: `unknown_string_escape_is_a_lex_error` FAILS (no error today — `\q` becomes `q`, then `main` type error). `valid_string_escapes_still_lex` should already pass.

- [ ] **Step 3: Reject unknown escapes in the string scanner**

In `apps/emet/src/lexer.rs`, `scan_string_segment`, change the escape `match` (was line ~416):

```rust
            let repl = match next {
                'n' => '\n',
                't' => '\t',
                '"' => '"',
                '\\' => '\\',
                other => {
                    return Err(LexError {
                        msg: format!("unknown string escape `\\{other}`"),
                        span: byte_at[i]..byte_at[(i + 2).min(chars.len())],
                    })
                }
            };
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p emet --test char_and_string`
Expected: PASS. Confirm existing interpolation/`\${`/`\u{}` tests still pass — those branches sit *above* this `match`, so they are untouched.

- [ ] **Step 5: Run the whole crate**

Run: `cargo test -p emet`
Expected: PASS. Check `apps/emet/tests/interpolation.rs` too (escapes near `${`).

- [ ] **Step 6: Commit**

```bash
git add apps/emet/src/lexer.rs apps/emet/tests/char_and_string.rs
git commit -m "fix(emet): reject unknown string escapes at the lexer

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** note in `scan_string_segment` that unknown escapes are now errors, matching the char-literal path (`\${` and `\u{...}` still handled above).

**Locked-assertion updates:** none in the catalog pin the old swallow behaviour. Two assertions added.

---

## Task 6: Reserved-word binding / bare-use check (#16, #19)

`keep n = n + 1` and `rollback { }` currently leak `t9 -> t10` / `{ } -> t2` type errors. `keep`/`rollback`/`retry`/`scroll` and the six glyph words are reserved constructors that must not be *bound* (as a decl head or `let` binder) nor *used bare* (as a plain variable). Detect these in the parser and produce a plain-language message.

**Files:**
- Modify: `apps/emet/src/parser.rs` (`value_item` / `decls_parser` `item` binding heads; a bare-use validation)
- Test: `apps/emet/tests/diagnostics.rs`

**Interfaces:**
- Consumes: `is_reserved_constructor` (existing).
- Produces: parse errors for a reserved word bound as a decl/`let` head, and for `keep`/`rollback`/`retry` used bare as a value. `aptPackage`/`file`/etc. already fail as bare values (they require `{…}`); the new coverage is the *binding-head* rejection plus the `keep`/`rollback`/`retry` bare-value case.

**Note:** The binding heads in `module_parser::value_item` and `decls_parser::item` use `ident()`, which does NOT exclude reserved words (only `expr_parser::var` excludes them). So `keep n = …` parses today. Add a reserved-word guard on the binding-head ident.

- [ ] **Step 1: Write the failing test**

Add to `apps/emet/tests/diagnostics.rs`:

```rust
#[test]
fn binding_a_reserved_word_is_rejected() {
    let e = err("keep n = n + 1\n\nmain = [ ]");
    assert_eq!(e.phase, Phase::Parse);
    assert!(e.msg.contains("`keep`"), "should name the word: {}", e.msg);
    assert!(e.msg.contains("reserved"), "should say reserved: {}", e.msg);
}

#[test]
fn braced_rollback_points_at_braceless_form() {
    let e = err(r#"main = [ scroll { name = "w", glyphs = [], policy = rollback { } } ]"#);
    assert_eq!(e.phase, Phase::Parse);
    assert!(e.msg.contains("without braces"), "msg: {}", e.msg);
}
```

`braced_rollback_points_at_braceless_form` already passes (the existing `build_constructor` `rollback`/`keep` arm returns "written without braces") — but that arm is reached as a *type* error today? Verify: `rollback { }` parses as a `constructor` atom (since `rollback` is reserved), then `build_constructor` errors with "written without braces" as a `Rich::custom` — a *parse* error. The corpus capture (#19) shows a *type* error `expected Policy, found { } -> t2`, meaning `rollback {` is NOT hitting `build_constructor` — `rollback` matches `policy_word` first (an atom), then `{ }` reads as a following application argument (a record). So the fix must ensure a braced reserved policy word is caught. Adjust the test if needed after Step 2's observation.

- [ ] **Step 2: Run tests to verify they fail (and observe the real path)**

Run: `cargo test -p emet --test diagnostics binding_a_reserved_word_is_rejected braced_rollback_points_at_braceless_form`
Expected: `binding_a_reserved_word_is_rejected` FAILS (`keep n = …` type-errors, not parse). Note the actual phase/msg of `braced_rollback` from the failure output to confirm which path it takes.

- [ ] **Step 3: Reject reserved words as binding heads**

In `apps/emet/src/parser.rs`, replace the binding-head `ident()` in both `module_parser::value_item` and `decls_parser::item` with a validated ident that rejects reserved words. Add near `ident`:

```rust
fn binding_head<'src, I>() -> impl Parser<'src, I, String, extra::Err<Rich<'src, Tok, TokSpan>>> + Clone
where
    I: ValueInput<'src, Token = Tok, Span = TokSpan>,
{
    select! { Tok::Ident(name) => name }.try_map(|name, span| {
        if is_reserved_constructor(&name) {
            Err(Rich::custom(
                span,
                format!("`{name}` is a reserved word and can't be used as a name to bind"),
            ))
        } else {
            Ok(name)
        }
    })
}
```

Change `value_item` (was `ident().map_with(...)`) and `decls_parser`'s `item` first component to use `binding_head()` in place of `ident()`.

- [ ] **Step 4: Handle braced `rollback`/`keep`**

Based on Step 2's observation: `policy_word` matches `rollback`/`keep` as an atom, and a following `{ }` becomes an application. To catch the braced form, make `policy_word` reject a directly-following `LBrace`. Change `policy_word` to peek for a brace and fail into `build_constructor`'s hint. Replace the `policy_word` definition body's `.map_with(...)` with a `.then` that rejects an adjacent brace:

```rust
        let policy_word = select! {
            Tok::Ident(name) if name == "rollback" || name == "keep" => name,
        }
        .then(just(Tok::LBrace).or_not().rewind())
        .try_map(|(name, braced), span| {
            if braced.is_some() {
                return Err(Rich::custom(
                    span,
                    format!("`{name}` is written without braces (e.g. `policy = {name}`)"),
                ));
            }
            let tag = if name == "rollback" {
                crate::ast::OnExhaustTag::Rollback
            } else {
                crate::ast::OnExhaustTag::Keep
            };
            Ok(Spanned(Expr::PolicyExhaust(tag), span_range(span)))
        });
```

If `.rewind()` is unavailable in this chumsky version, instead remove `rollback`/`keep` from `policy_word` and route them through `constructor_name` + `build_constructor` (which already emits "written without braces"), then add a braceless-atom alternative. Prefer the smallest change that makes `braced_rollback_points_at_braceless_form` assert `without braces` as a `Phase::Parse` error.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p emet --test diagnostics`
Expected: PASS. Confirm `let_without_in`/`lambda`/`field_access` still hold.

- [ ] **Step 6: Run the whole crate**

Run: `cargo test -p emet`
Expected: PASS. Confirm `scrolls.rs`, `recursive_scroll.rs`, `files.rs` (which build `scroll`/`file`/`retry`) still pass — reserved words remain usable as constructors, only rejected as binding heads / braced policy words.

- [ ] **Step 7: Commit**

```bash
git add apps/emet/src/parser.rs apps/emet/tests/diagnostics.rs
git commit -m "feat(emet): reject reserved words as binding heads and braced policy words

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** document `binding_head` and the braced-policy rejection (why `ident()` alone let reserved heads through; the build/match split of ADR 0017).

**Locked-assertion updates:** none broken. Two assertions added.

---

## Task 7: Targeted syntax hints (#12 `=>`, #13 missing `of`, #25 missing `in`, #23/#24 lambda `->`, #29 empty field, #04 missing `=`, #11 empty `case`)

Each is a small message improvement at the point the current grammar already fails. Deliver them as message rewrites keyed on the failing token, without adding recovery. Bundle the seven because each is a one-to-few-line change with a shared test surface.

**Files:**
- Modify: `apps/emet/src/parser.rs` (`humanize_expected`: token-directed hints; the empty-`case` special-case in `infer.rs`)
- Modify: `apps/emet/src/infer.rs` (empty-`case` #11)
- Test: `apps/emet/tests/diagnostics.rs`

**Interfaces:**
- Consumes: `humanize_expected` (Task 3).
- Produces: `humanize_expected` gains a `found`-token-directed hint clause; `check_exhaustive` (or `Expr::Case` inference) special-cases zero arms.

**Approach:** `humanize_expected` currently sees only the rendered string, which includes `found '<tok>'`. Extend it to parse the `found '<tok>'` prefix and, for specific found-tokens, append a targeted hint:
- found `'=>'` → hint `Case arms use '->', not '=>'.`
- found `case` where `of` is expected → the AUDIT #13 case; when the raw string contains `found 'case'` and the source has no `of`, append `A 'case' needs 'of' before its arms.` (Detect via the expected set not containing a useful token; keep the hint generic.)
- found `'\'` (lambda) → `A lambda is written '\x -> body'.`
- `let` without `in` → append `A 'let' needs 'in': 'let x = 1 in x'.`
- found `','` right after `=` (empty record field) → `This field has no value: write 'name = <expr>'.`
- found `'+'`/operator where `'='` expected (#04) → `This looks like a definition missing its '='.`

Because the humanizer lacks source context, key these purely on the `found`/`expected` tokens present in the raw string. Keep hints conservative: only append when the trigger token is unambiguous.

- [ ] **Step 1: Write the failing tests**

Add to `apps/emet/tests/diagnostics.rs`:

```rust
#[test]
fn arrow_typo_hint() {
    let e = err("f x =\n  case x of\n    1 => \"a\"\n    _ => \"b\"\n\nmain = f 1");
    assert_eq!(e.phase, Phase::Parse);
    assert!(e.msg.contains("=>") && e.msg.contains("->"), "msg: {}", e.msg);
}

#[test]
fn missing_equals_hint() {
    let e = err("f x x + 1\n\nmain = f 2");
    assert_eq!(e.phase, Phase::Parse);
    assert!(e.msg.contains("="), "should mention '=': {}", e.msg);
    assert!(e.msg.to_lowercase().contains("definition") || e.msg.contains("'='"), "msg: {}", e.msg);
}

#[test]
fn empty_case_is_reported_as_no_arms() {
    let e = err("f x =\n  case x of\n\nmain = f 1");
    assert_eq!(e.phase, Phase::Type);
    assert!(e.msg.contains("no arms") || e.msg.contains("at least one"), "msg: {}", e.msg);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p emet --test diagnostics arrow_typo_hint missing_equals_hint empty_case_is_reported_as_no_arms`
Expected: all FAIL.

- [ ] **Step 3: Add found-token hints to `humanize_expected`**

In `apps/emet/src/parser.rs`, at the end of `humanize_expected`, before returning, compute a `found_hint` from the raw string:

```rust
    let found_hint = if raw.contains("found '=>'") {
        " — case arms use '->', not '=>'"
    } else if raw.contains("found '\\'") {
        " — a lambda is written '\\x -> body'"
    } else if items.iter().any(|i| i == "'='") {
        " — this looks like a definition missing its '='"
    } else {
        ""
    };
    format!("{head}expected {joined}{close_hint}{found_hint}")
```

(Adjust the final `format!` from Task 4 to include `{found_hint}`.)

- [ ] **Step 4: Special-case the empty `case` (#11)**

In `apps/emet/src/infer.rs`, in the `Expr::Case` arm of `infer_expr_inner` (was ~line 1267), before `check_exhaustive`, add:

```rust
            if arms.is_empty() {
                return Err(TypeError::new(
                    "this `case` has no arms — a `case … of` needs at least one `pattern -> expression` arm",
                    span.clone(),
                ));
            }
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p emet --test diagnostics`
Expected: PASS.

- [ ] **Step 6: Run the whole crate**

Run: `cargo test -p emet`
Expected: PASS. Note: `pattern_matching.rs` non-exhaustive assertions are on *non-empty* cases, so the empty-case guard does not intercept them. Confirm.

- [ ] **Step 7: Commit**

```bash
git add apps/emet/src/parser.rs apps/emet/src/infer.rs apps/emet/tests/diagnostics.rs
git commit -m "feat(emet): targeted syntax hints for =>, missing =, and empty case

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** document each `found_hint` trigger and the empty-`case` guard (why it precedes the exhaustiveness pass — #11 wants "any arm", not "a catch-all").

**Locked-assertion updates:** none broken. `pattern_matching.rs` (86, 156) unaffected. Three assertions added. The `missing of` (#13), `missing in` (#25), lambda `->` (#23/#24), and empty-field (#29) hints ride the humanizer conservatively; add tests for any that the humanizer reliably fires on (verify against the corpus in Task 15 rather than over-asserting here).

---

## Task 8: Plain-language constraint messages + reversed framing (#31/#32/#39/#42/#45/#46, #41)

`type 'String' does not satisfy 'number'` is internal vocabulary. Rewrite the constraint-mismatch message to name the concrete type in plain language, and fix the two reversed cases: #46 (`Bool does not satisfy number` — the condition should be Bool) and #41 (`expected String, found Policy` reads backward for a Policy field).

**Files:**
- Modify: `apps/emet/src/infer.rs` (`bind` constraint message; the `Expr::If` condition unify; the `scroll` `policy` unify)
- Test: `apps/emet/tests/diagnostics.rs`

**Interfaces:**
- Consumes: `render_type` (Task 1), `constraint_name` (existing).
- Produces: a `number`-constraint failure phrased as `this needs to be a number, but it's a <Type>`; an `if`-condition failure phrased around Bool; a `policy` field failure phrased around Policy.

- [ ] **Step 1: Write the failing tests**

Add to `apps/emet/tests/diagnostics.rs`:

```rust
#[test]
fn number_constraint_is_plain_language() {
    let e = err(r#"main = [ ]
x = 1 + "two""#);
    assert_eq!(e.phase, Phase::Type);
    assert!(!e.msg.contains("satisfy"), "jargon leaked: {}", e.msg);
    assert!(e.msg.to_lowercase().contains("number"), "msg: {}", e.msg);
    assert!(e.msg.contains("String"), "should name the offending type: {}", e.msg);
}

#[test]
fn if_condition_must_be_bool() {
    let e = err("main = [ ]\ny = if 1 then 2 else 3");
    assert_eq!(e.phase, Phase::Type);
    assert!(e.msg.contains("Bool"), "should mention Bool: {}", e.msg);
    assert!(e.msg.to_lowercase().contains("condition"), "msg: {}", e.msg);
}

#[test]
fn policy_field_wants_a_policy() {
    let e = err(r#"main = [ scroll { name = "w", glyphs = [], policy = "aggressive" } ]"#);
    assert_eq!(e.phase, Phase::Type);
    assert!(e.msg.contains("Policy"), "should mention Policy: {}", e.msg);
    assert!(!e.msg.contains("expected `String`"), "reversed framing leaked: {}", e.msg);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p emet --test diagnostics number_constraint_is_plain_language if_condition_must_be_bool policy_field_wants_a_policy`
Expected: all FAIL — current messages use `satisfy` / reversed framing.

- [ ] **Step 3: Rephrase the constraint message**

In `apps/emet/src/infer.rs` `bind` (was line ~372), for the `Number` case give plain wording; keep a generic form for the others:

```rust
        if !constraint_admits(c, &t) {
            let rendered = render_type(&self.apply(&t));
            let msg = match c {
                Constraint::Number => format!("this needs to be a number (Int or Float), but it's a `{rendered}`"),
                Constraint::Comparable => format!("this needs to be comparable (Int, Float, or String), but it's a `{rendered}`"),
                Constraint::Appendable => format!("this needs to be appendable (String or List), but it's a `{rendered}`"),
                Constraint::None => format!("type `{rendered}` is not allowed here"),
            };
            return Err(TypeError::new(msg, span.clone()));
        }
```

- [ ] **Step 4: Give the `if` condition its own message**

In `apps/emet/src/infer.rs` `Expr::If` (was line ~1256), replace the plain `unify` with a contextual error:

```rust
        Expr::If { cond, then_, else_ } => {
            let ct = infer_expr(inf, env, cond)?;
            inf.unify(&ct, &con("Bool"), &cond.1).map_err(|_| {
                TypeError::new(
                    format!(
                        "the condition of an `if` must be a `Bool`, but this is a `{}`",
                        render_type(&inf.apply(&ct))
                    ),
                    cond.1.clone(),
                )
            })?;
```

(Keep the rest of the arm unchanged.)

- [ ] **Step 5: Give the `policy` field its own message**

In `apps/emet/src/infer.rs` `Expr::Scroll` (was line ~1143), replace the policy `unify`:

```rust
            if let Some(p) = policy {
                let pt = infer_expr(inf, env, p)?;
                inf.unify(&pt, &con("Policy"), &p.1).map_err(|_| {
                    TypeError::new(
                        format!(
                            "the `policy` field takes a `Policy` (from `keep`/`rollback`/`retry`), but this is a `{}`",
                            render_type(&inf.apply(&pt))
                        ),
                        p.1.clone(),
                    )
                })?;
            }
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p emet --test diagnostics`
Expected: PASS.

- [ ] **Step 7: Run the whole crate**

Run: `cargo test -p emet`
Expected: PASS. `diagnostics.rs::wrong_field_type_is_type_error` and `signature_conflict_is_type_error` assert `type mismatch` — those still flow through `unify`'s Con-mismatch arm (unchanged), so they hold. `appendable.rs` has no message-substring assertions (verified), so it is unaffected; the `merge_constraints` `no type satisfies both` path (infer.rs ~349) is untouched by this task.

- [ ] **Step 8: Commit**

```bash
git add apps/emet/src/infer.rs apps/emet/tests/diagnostics.rs
git commit -m "feat(emet): plain-language constraint messages; fix reversed if/policy framing

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** document why the `if`-condition and `policy` unifies wrap their errors (the direction of `unify`'s expected/found is confusing at these sites; the constraint vocabulary is internal).

**Locked-assertion updates:** none. `appendable.rs` has no message-substring assertions (verified); the `merge_constraints` `no type satisfies both` path is unchanged. Three assertions added.

---

## Task 9: did-you-mean for unbound names, constructors, and type constructors (#35/#36/#37, #57/#58/#63)

Add edit-distance suggestions. `unknown name 'greetnig'` → append `— did you mean 'greeting'?`; same for `unknown constructor` and `unknown type constructor`. For qualified names (#37 `List.mpa`) fix the misleading note and suggest over the dotted set. For `Str`/`Glyphs` (#63) append the removed-alias note.

**Files:**
- Modify: `apps/emet/src/infer.rs` (add `edit_distance` + `did_you_mean`; the unbound-name, `Expr::Ctor`, `Pattern::Ctor`, and `validate_type_refs_inner` unknown sites)
- Test: `apps/emet/tests/diagnostics.rs`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `fn edit_distance(a: &str, b: &str) -> usize` (Levenshtein) and `fn did_you_mean(target: &str, candidates: impl Iterator<Item = String>) -> Option<String>` in `infer.rs`, returning the closest candidate within a small threshold (`<= 2`, and shorter than the target's length).

- [ ] **Step 1: Write the failing tests**

Add to `apps/emet/tests/diagnostics.rs`:

```rust
#[test]
fn unbound_name_suggests_nearest() {
    let e = err("greeting = \"hi\"\nmain = greetnig");
    assert_eq!(e.phase, Phase::Type);
    assert!(e.msg.contains("greeting"), "should suggest greeting: {}", e.msg);
    assert!(e.msg.contains("did you mean"), "msg: {}", e.msg);
}

#[test]
fn unknown_constructor_suggests_nearest() {
    let e = err("main = Nothin");
    assert_eq!(e.phase, Phase::Type);
    assert!(e.msg.contains("Nothing"), "should suggest Nothing: {}", e.msg);
}

#[test]
fn unknown_type_constructor_suggests_nearest() {
    let e = err("f : Strng\nf = \"x\"\nmain = [ ]");
    assert_eq!(e.phase, Phase::Type);
    assert!(e.msg.contains("String"), "should suggest String: {}", e.msg);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p emet --test diagnostics unbound_name_suggests_nearest unknown_constructor_suggests_nearest unknown_type_constructor_suggests_nearest`
Expected: all FAIL.

- [ ] **Step 3: Add the edit-distance helpers**

Add to `apps/emet/src/infer.rs`:

```rust
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

fn did_you_mean(target: &str, candidates: impl Iterator<Item = String>) -> Option<String> {
    let mut best: Option<(usize, String)> = None;
    for cand in candidates {
        let d = edit_distance(target, &cand);
        let threshold = if target.len() <= 4 { 1 } else { 2 };
        if d == 0 || d > threshold {
            continue;
        }
        if best.as_ref().map(|(bd, _)| d < *bd).unwrap_or(true) {
            best = Some((d, cand));
        }
    }
    best.map(|(_, name)| name)
}
```

- [ ] **Step 4: Wire suggestions into the unbound-name site**

In `apps/emet/src/infer.rs` `Expr::Var` (was line ~1080). `TyEnv` exposes `pub fn entries(&self) -> impl Iterator<Item = (&String, &Scheme)>` (infer.rs:1044); use it for candidates:

```rust
            None => {
                let mut msg = format!("unknown name `{name}`");
                if let Some(hint) = did_you_mean(name, env.entries().map(|(k, _)| k.clone())) {
                    msg.push_str(&format!(" — did you mean `{hint}`?"));
                }
                let note = if name.contains('.') {
                    "this is a module-qualified name; check the module and member spelling"
                } else {
                    "not bound by any declaration, `let`, or lambda parameter"
                };
                Err(TypeError::new(msg, span.clone()).note(note))
            }
```

For `Expr::Ctor`, filter `env.entries()` to uppercase-initial keys.

- [ ] **Step 5: Wire suggestions into the constructor sites**

`Expr::Ctor` (was line ~1091) and `Pattern::Ctor` (was line ~1373): both look up over `env`/`constructor_scheme`. For `Expr::Ctor`, suggest over `env.names()` filtered to uppercase-initial. For unknown type constructors in `validate_type_refs_inner` (was line ~1958), suggest over `type_arities.keys()`:

```rust
            None => {
                let mut msg = format!("unknown type constructor `{name}`");
                if let Some(hint) = did_you_mean(name, type_arities.keys().cloned()) {
                    msg.push_str(&format!(" — did you mean `{hint}`?"));
                }
                if name == "Str" {
                    msg.push_str(" (`Str` was removed; the type is now `String`)");
                }
                if name == "Glyphs" {
                    msg.push_str(" (`Glyphs` was removed; the type is now `List Glyph`)");
                }
                Err(TypeError::new(msg, span.clone()))
            }
```

For `Expr::Ctor`, mirror with the env-uppercase candidate set. For `Pattern::Ctor` unknown, mirror using `constructor_scheme` candidates (find the ctor-name source — `user_ctor_schemes.keys()` plus the prelude constructor names).

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p emet --test diagnostics`
Expected: PASS.

- [ ] **Step 7: Run the whole crate**

Run: `cargo test -p emet`
Expected: PASS. `diagnostics.rs::str_alias_is_unknown_type_constructor` (line 200) and `glyphs_alias_is_unknown_type_constructor` (208) still assert `unknown type constructor` + `` `Str` ``/`` `Glyphs` `` — those substrings survive (we *append*, not replace). `modules.rs` `unknown constructor` (137) / `unknown type constructor` (149) also survive.

- [ ] **Step 8: Commit**

```bash
git add apps/emet/src/infer.rs apps/emet/tests/diagnostics.rs
git commit -m "feat(emet): did-you-mean suggestions for unknown names, constructors, and type constructors

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** document `edit_distance`/`did_you_mean` (Levenshtein, threshold by target length; no new dependency) and the qualified-name note fix.

**Locked-assertion updates:** none broken (all are `contains`, and suggestions append). `diagnostics.rs` lines 203/204/211/212, `modules.rs` 137/149 verified green. Three assertions added.

---

## Task 10: Duplicate-binding detection, top-level and `let` (#61)

`x = 1` then `x = 2` is silently ignored; only the incidental `main` type error shows. Detect a repeated binding name when collecting top-level decls (`fold_decls`) and `let` decls. Emit a parse error naming the duplicated name.

**Files:**
- Modify: `apps/emet/src/parser.rs` (`fold_decls`)
- Test: `apps/emet/tests/diagnostics.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: a `ParseError` for a duplicate binding name within one decl group (top-level or one `let`). `fold_decls` gains a seen-set.

**Note:** `fold_decls` runs for both `let` (via `decls_parser` → `Expr::Let` `try_map`) and top-level (via `parse`). Adding the check there covers both. A signature + its binding share a name legitimately — only two *bindings* of the same name conflict, so track binding names only.

- [ ] **Step 1: Write the failing tests**

Add to `apps/emet/tests/diagnostics.rs`:

```rust
#[test]
fn duplicate_top_level_binding_is_rejected() {
    let e = err("x = 1\nx = 2\n\nmain = [ ]");
    assert_eq!(e.phase, Phase::Parse);
    assert!(e.msg.contains("`x`"), "should name x: {}", e.msg);
    assert!(e.msg.contains("twice") || e.msg.contains("defined"), "msg: {}", e.msg);
}

#[test]
fn duplicate_let_binding_is_rejected() {
    let e = err("main =\n  let x = 1\n      x = 2\n  in [ ]");
    assert_eq!(e.phase, Phase::Parse);
    assert!(e.msg.contains("`x`"), "should name x: {}", e.msg);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p emet --test diagnostics duplicate_top_level_binding_is_rejected duplicate_let_binding_is_rejected`
Expected: both FAIL (currently the second binding silently wins; error is a `main` type error, wrong phase).

- [ ] **Step 3: Add the duplicate check to `fold_decls`**

In `apps/emet/src/parser.rs` `fold_decls`, track seen binding names. In the `DeclItem::Bind` arm, before `out.push(...)`:

```rust
                if out.iter().any(|d: &Decl| d.name == name) {
                    return Err(ParseError {
                        msg: format!("`{name}` is defined twice — remove or rename one"),
                        span,
                    });
                }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p emet --test diagnostics`
Expected: PASS.

- [ ] **Step 5: Run the whole crate**

Run: `cargo test -p emet`
Expected: PASS. Mutual-recursion and multi-decl tests (`mutual_recursion.rs`) use *distinct* names, so they hold. Confirm no fixture legitimately redefines a name.

- [ ] **Step 6: Commit**

```bash
git add apps/emet/src/parser.rs apps/emet/tests/diagnostics.rs
git commit -m "fix(emet): reject duplicate bindings at top level and in let

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** document the duplicate-name guard in `fold_decls` (why binding names only, not signatures; that it covers both top-level and `let` since both route through `fold_decls`).

**Locked-assertion updates:** none broken. Two assertions added.

---

## Task 11: Reject the "not yet supported here" gaps as parse errors (#26 let-in-case-arm, #27 negative-literal argument)

Both currently fall through to `main must be List Scroll but is Int`. The real cause is a documented parser gap (`docs/TODO.md`). Item 15 becomes two *specific* parse errors naming the unsupported form — NOT full support.

**Files:**
- Modify: `apps/emet/src/parser.rs` (case-arm parser for #26; application parser for #27)
- Test: `apps/emet/tests/diagnostics.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: a `Phase::Parse` error for `let … in` inside a case arm, and for a bare negative-literal *argument* (`f x -1`), each naming the unsupported form.

**Scope honesty:** these are targeted rejections, not new grammar. #26: the `arm` parser is `pattern -> expr`; a `let … in` body inside an arm currently mis-layouts. #27: `f x -1` parses `-` as subtraction (`f x - 1`), so `f x -1` silently means `sub (f x) 1`; the "unsupported" framing is the negative-literal-*argument* ambiguity. Verify each corpus case's *actual* current failure in Step 2 and target the smallest rejection that names the form. If a clean parse-time rejection is infeasible for #27 without disturbing subtraction, deliver #26 only and record #27 as still-deferred here and in `docs/TODO.md` — do not force it.

- [ ] **Step 1: Write the failing tests**

Add to `apps/emet/tests/diagnostics.rs`:

```rust
#[test]
fn let_in_a_case_arm_is_reported_as_unsupported() {
    let e = err("f x =\n  case x of\n    1 ->\n      let y = 2 in y\n    _ -> 0\n\nmain = f 1");
    assert_ne!(e.span, 0..0, "should locate the arm, not the module: {e:?}");
    assert!(
        e.msg.to_lowercase().contains("let") || e.msg.to_lowercase().contains("not yet"),
        "should name the unsupported form: {}",
        e.msg
    );
}
```

- [ ] **Step 2: Run test to verify it fails, and observe the current parse**

Run: `cargo test -p emet --test diagnostics let_in_a_case_arm_is_reported_as_unsupported`
Expected: FAIL — currently a `main` type error at `0..0`. Read the failure to see the actual token stream / layout behaviour for the arm.

- [ ] **Step 3: Detect and reject `let … in` in an arm body**

In `apps/emet/src/parser.rs`, the `arm` parser is `pattern_parser().then_ignore(just(Tok::Arrow)).then(expr.clone())`. A `let` body currently parses but the layout produces a bad tree. Wrap the arm body so a leading `Tok::Let` in an arm is a targeted rejection:

```rust
        let arm = pattern_parser()
            .then_ignore(just(Tok::Arrow))
            .then(
                just(Tok::Let)
                    .rewind()
                    .ignore_then(any())
                    .try_map(|_, span| {
                        Err::<Spanned<Expr>, _>(Rich::custom(
                            span,
                            "`let … in` inside a `case` arm is not yet supported here — lift the binding out of the arm",
                        ))
                    })
                    .or(expr.clone()),
            )
            .map(|(pat, body)| Arm { pat, body });
```

If `.rewind()` is unavailable, use a `choice` where the first branch matches `just(Tok::Let)` and `try_map`s to the error. Confirm the arm still parses ordinary bodies.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p emet --test diagnostics let_in_a_case_arm_is_reported_as_unsupported`
Expected: PASS.

- [ ] **Step 5: Run the whole crate**

Run: `cargo test -p emet`
Expected: PASS. Confirm `pattern_matching.rs` and any `case`-with-let fixtures still work — if a legitimate multi-line arm used `let`, that was already broken (it's the gap), so no regression.

- [ ] **Step 6: Handle #27 or record it deferred**

Attempt a parse-time rejection of a negative-literal *argument* only if it does not disturb subtraction. If infeasible, add a line to `docs/TODO.md` under known gaps: "negative-literal argument `f x -1` still surfaces as a downstream type error; needs application/subtraction disambiguation (AUDIT #27)." Do not write a test for #27 if not implemented.

- [ ] **Step 7: Commit**

```bash
git add apps/emet/src/parser.rs apps/emet/tests/diagnostics.rs docs/TODO.md
git commit -m "feat(emet): reject let-in-case-arm with a specific unsupported-form error

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** document the arm-body `let` rejection (why targeted, not supported; points at ADR 0032 for the broader recovery). Note #27's disposition.

**Locked-assertion updates:** none broken. One assertion added (#26); #27 deferred if infeasible.

---

## Task 12: Angle-bracket and empty scroll-name rejection (#22)

`scroll { name = "<removes>" }` compiles clean (exit 0) — a silent wrong-accept. Reject a scroll name that contains angle brackets or is empty, at eval time (where the concrete name string is known). This needs the name's span; use the `scroll` name expr span.

**Files:**
- Modify: `apps/emet/src/eval.rs` (`EvalError` gains a `span`; the `Expr::Scroll` name check); `apps/emet/src/lib.rs` (thread the eval span — coordinate with Task 14)
- Test: `apps/emet/tests/scrolls.rs`

**Interfaces:**
- Consumes: nothing yet (Task 14 will consume the same `EvalError.span`).
- Produces: `EvalError { msg, span }` — this task ADDS the `span: Span` field to `EvalError` (default `0..0`) and sets it for the scroll-name rejection. Task 14 threads it through to `Error`.

**Ordering note:** this task introduces the `EvalError.span` field that Task 14 relies on. Keep the field addition minimal here (add the field, default it to `0..0` at every construction site, populate it only for the new name check); Task 14 does the general threading.

- [ ] **Step 1: Write the failing tests**

Add to `apps/emet/tests/scrolls.rs`:

```rust
#[test]
fn angle_bracket_scroll_name_is_rejected() {
    let e = match emet::compile(r#"main = [ scroll { name = "<removes>", glyphs = [] } ]"#) {
        Ok(_) => panic!("angle-bracket name should be rejected"),
        Err(e) => e,
    };
    assert!(e.msg.contains("angle bracket") || e.msg.contains("<"), "msg: {}", e.msg);
    assert!(e.msg.contains("name"), "msg: {}", e.msg);
}

#[test]
fn empty_scroll_name_is_rejected() {
    let e = match emet::compile(r#"main = [ scroll { name = "", glyphs = [] } ]"#) {
        Ok(_) => panic!("empty name should be rejected"),
        Err(e) => e,
    };
    assert!(e.msg.contains("name"), "msg: {}", e.msg);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p emet --test scrolls angle_bracket_scroll_name_is_rejected empty_scroll_name_is_rejected`
Expected: both FAIL (compilation succeeds today).

- [ ] **Step 3: Add `span` to `EvalError`**

In `apps/emet/src/eval.rs`, change:

```rust
pub struct EvalError {
    pub msg: String,
    pub span: Span,
}
```

Add `use crate::lexer::Span;` if not already imported (grep). At every `EvalError { msg: … }` construction (grep `EvalError {` — the `perms_from_mode` sites near line 628/632, the `apply` site near 490), add `span: 0..0,`.

- [ ] **Step 4: Reject bad scroll names in eval**

In `apps/emet/src/eval.rs` `Expr::Scroll` (was line ~195), after `let name = as_str(...)`:

```rust
            let name = as_str(eval(env, name_expr, depth)?);
            if name.is_empty() || name.contains('<') || name.contains('>') {
                return Err(EvalError {
                    msg: format!("scroll name `{name}` is not a valid host identifier (no angle brackets, not empty)"),
                    span: name_expr.1.clone(),
                });
            }
```

(Rename the destructured `name` binding for the expr to `name_expr` so its `.1` span is reachable; adjust the match pattern accordingly.)

- [ ] **Step 5: Surface the eval span in `lib.rs`**

In `apps/emet/src/lib.rs` `compile_all` (was line ~134), change the eval error mapping to carry the span:

```rust
    let scrolls = eval::run_module(&module).map_err(|e| {
        vec![Error {
            phase: Phase::Analyze,
            msg: e.msg,
            span: e.span,
            note: None,
        }]
    })?;
```

Do the same in `resolve.rs` if it maps `EvalError` (grep `EvalError`/`run_module` in `resolve.rs`).

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p emet --test scrolls`
Expected: PASS. Existing scroll tests (name = "web") still compile.

- [ ] **Step 7: Run the whole crate**

Run: `cargo test -p emet`
Expected: PASS. Grep the corpus/examples for any scroll name with `<`/`>`/empty that a repo example relies on — there should be none.

- [ ] **Step 8: Commit**

```bash
git add apps/emet/src/eval.rs apps/emet/src/lib.rs apps/emet/src/resolve.rs apps/emet/tests/scrolls.rs
git commit -m "fix(emet): reject angle-bracket and empty scroll names at eval

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** document the scroll-name validation (closes the wrong-accept from `docs/TODO.md`) and the new `EvalError.span`.

**Locked-assertion updates:** none broken. Two assertions added. Remove the `docs/TODO.md` known-gap line for angle-bracket names if present (grep and update; stage `docs/TODO.md` too if edited).

---

## Task 13: `retry` unknown-field valid-field list + `both-vs-neither` scroll hint (#21, #18)

Two small wording adds. #21: `unknown 'retry' field 'bogus'` should list the valid retry fields. #18 (trivial-only): the `scroll` "exactly one of glyphs or groups" message is already A-grade; only add a both-vs-neither distinction if it is a one-line change.

**Files:**
- Modify: `apps/emet/src/infer.rs` (`Expr::PolicyRetry` unknown-field); `apps/emet/src/parser.rs` (`build_constructor` scroll contents — the both-vs-neither branch)
- Test: `apps/emet/tests/recursive_scroll.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: an enriched `unknown 'retry' field` message; optionally a both-vs-neither split in the scroll message.

- [ ] **Step 1: Write the failing test**

Add to `apps/emet/tests/recursive_scroll.rs`:

```rust
#[test]
fn unknown_retry_field_lists_valid_fields() {
    let src = r#"main = [ scroll { name = "w", glyphs = [], policy = retry { maxAttempts = 3, bogus = 1 } } ]"#;
    let e = match emet::compile(src) {
        Ok(_) => panic!("expected an error"),
        Err(e) => e,
    };
    assert!(e.msg.contains("bogus"), "msg: {}", e.msg);
    assert!(e.msg.contains("maxAttempts"), "should list valid fields: {}", e.msg);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p emet --test recursive_scroll unknown_retry_field_lists_valid_fields`
Expected: FAIL — message names only `bogus`.

- [ ] **Step 3: Enrich the retry unknown-field message**

In `apps/emet/src/infer.rs` `Expr::PolicyRetry` (was line ~1177):

```rust
                    other => {
                        return Err(TypeError::new(
                            format!(
                                "unknown `retry` field `{other}` — valid fields are maxAttempts, baseDelayMs, backoffMultiplier, maxDelayMs, jitterFraction, maxElapsedMs, onExhaust"
                            ),
                            value.1.clone(),
                        ))
                    }
```

- [ ] **Step 4: both-vs-neither (only if trivial)**

In `apps/emet/src/parser.rs` `build_constructor` scroll `contents` match, the `_` arm handles both "both present" and "neither present". Split only if trivial:

```rust
            let contents = match (glyphs, groups) {
                (Some(g), None) => ContentsExpr::Glyphs(Box::new(g)),
                (None, Some(g)) => ContentsExpr::Groups(Box::new(g)),
                (Some(_), Some(_)) => {
                    return Err(Rich::custom(
                        span,
                        "`scroll` has both `glyphs` and `groups` — use exactly one".to_string(),
                    ))
                }
                (None, None) => {
                    return Err(Rich::custom(
                        span,
                        "`scroll` needs exactly one of `glyphs` or `groups`".to_string(),
                    ))
                }
            };
```

Both messages retain the substring `exactly one` … except the both-case. The locked assertions (`recursive_scroll.rs` 62/74, `scrolls.rs` 113) require `exactly one of 'glyphs' or 'groups'`. Test 62 is the *both* case — so its message MUST keep that phrase. Adjust the both-branch message to keep it:

```rust
                (Some(_), Some(_)) => {
                    return Err(Rich::custom(
                        span,
                        "`scroll` has both `glyphs` and `groups`, but needs exactly one of `glyphs` or `groups`".to_string(),
                    ))
                }
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p emet --test recursive_scroll --test scrolls`
Expected: PASS — `recursive_scroll.rs` 62/74 (`exactly one of 'glyphs' or 'groups'`) and `scrolls.rs` 113 still hold; the new retry test passes.

- [ ] **Step 6: Run the whole crate**

Run: `cargo test -p emet`
Expected: PASS. `recursive_scroll.rs::an_unknown_retry_field_is_a_type_error` (141) asserts `retrys` — that is a *different* case (a `scroll` field typo `retrys`, not a retry-knob typo); this task does not touch it. Confirm it stays green.

- [ ] **Step 7: Commit**

```bash
git add apps/emet/src/infer.rs apps/emet/src/parser.rs apps/emet/tests/recursive_scroll.rs
git commit -m "feat(emet): list valid retry fields; split scroll both-vs-neither message

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** document the retry valid-field list and the both-vs-neither split (why the `exactly one` phrase is preserved in both — locked by `recursive_scroll.rs`).

**Locked-assertion updates:** `recursive_scroll.rs` 62/74 preserved (`exactly one of 'glyphs' or 'groups'` kept in the both-branch). `scrolls.rs` 113 preserved. `recursive_scroll.rs` 141 (`retrys`) untouched. One assertion added.

---

## Task 14: Thread source spans through eval → analyze (#50, and #26/#27/#08/#61 locations)

The IR drops spans, so analysis errors (#50 conflicting keys) and eval errors pin to `1:1` or the `main` decl. Thread the offending glyph's / scroll's source span from eval into `analyze` so a conflicting-key error underlines the second glyph, and the `main`-type error locates the `main` binding. This is the final, cross-cutting task; scope it honestly.

**Files:**
- Modify: `apps/emet/src/ir.rs` (or `scroll-format` — see scope note), `apps/emet/src/eval.rs`, `apps/emet/src/lib.rs` (`analyze` signature + call), `apps/emet/src/infer.rs` (`finish_main` span)
- Test: `apps/emet/tests/scrolls.rs`, `apps/emet/tests/diagnostics.rs`

**Interfaces:**
- Consumes: `EvalError.span` (Task 12).
- Produces: `analyze` returning a spanned error (a `Span` alongside the conflict `msg`), and `finish_main` locating the `main` binding.

**SCOPE — read before starting.** The AUDIT calls the IR-drops-spans problem "mildly architectural". The `Glyph`/`Scroll` IR lives in the shared `scroll-format` crate (the wire contract) — its field/variant order is versioned; **do not add span fields to the wire types.** Instead, thread spans through a *side channel* kept in `emet` only:

1. **Feasible here (do this):**
   - **#50 conflicting keys:** `eval::run_module` already walks scroll/glyph exprs which carry spans. Build an auxiliary `Vec<(String /*glyph key*/, Span)>` per leaf during eval, or have `eval` return spans alongside scrolls in an emet-side wrapper, and pass it to `analyze`. Change `analyze(&[Scroll])` to also accept the key→span map so the conflict error carries the second glyph's span. Keep the wire `Scroll`/`Glyph` unchanged.
   - **`main`-type error location (#08/#26/#27/#61 collapse):** `finish_main` uses `0..0`. Thread the `main` decl's span: `check_module`/`finish_main` has access to the module; look up the `main` `Decl`'s `span` and use it instead of `0..0`. This alone moves the `main`-type errors off `1:1` onto the `main` binding — a real improvement even though it doesn't reach the true syntactic cause (that stays with #26/#27's Task 11 rejection).

2. **Out of feasible scope (record, do not attempt):** general per-IR-node span provenance for *every* analysis. If the conflicting-key span channel proves to require touching more than `emet`'s `ir.rs`/`eval.rs`/`lib.rs`, implement the `main`-span half (which is self-contained) and record the conflict-span half as a follow-up referencing ADR 0032. Split honestly at the commit boundary.

- [ ] **Step 1: Write the failing tests**

Add to `apps/emet/tests/scrolls.rs`:

```rust
#[test]
fn conflicting_keys_span_points_at_a_glyph_not_module_start() {
    let src = r#"main : List Scroll
main =
  [ scroll
      { name = "web"
      , glyphs =
          [ file { path = "/etc/motd", contents = "hello", mode = "0644" }
          , file { path = "/etc/motd", contents = "goodbye", mode = "0644" }
          ]
      }
  ]
"#;
    let e = match emet::compile(src) {
        Ok(_) => panic!("expected a conflict"),
        Err(e) => e,
    };
    assert!(e.msg.contains("/etc/motd"), "msg: {}", e.msg);
    assert_ne!(e.span, 0..0, "span must not be the module-start sentinel: {e:?}");
}
```

Add to `apps/emet/tests/diagnostics.rs`:

```rust
#[test]
fn main_type_error_locates_the_main_binding() {
    let e = err("main = 5");
    assert_eq!(e.phase, Phase::Type);
    assert!(e.msg.contains("List Scroll"), "msg: {}", e.msg);
    assert_ne!(e.span, 0..0, "should locate the main binding, not 0..0: {e:?}");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p emet --test scrolls conflicting_keys_span_points_at_a_glyph_not_module_start && cargo test -p emet --test diagnostics main_type_error_locates_the_main_binding`
Expected: both FAIL — spans are `0..0`.

- [ ] **Step 3: Locate the `main` binding span in `finish_main`**

In `apps/emet/src/infer.rs`, thread the module (or a `main_span: Span`) into `finish_main`. `check_module` (was line ~2213) has the `Module`; find the `main` `Decl`'s `span` and pass it. Change `finish_main`'s two `0..0` sites (the `no main` error and the mismatch error) to use `main_span` when available. For the mismatch:

```rust
        None => Err(TypeError::new(
            format!("`main` must be `List Scroll` (a list of scrolls), but is `{}`", render_type(&main_ty)),
            main_span.clone(),
        )),
```

Find `main_span` by locating `m.decls.iter().find(|d| d.name == "main").map(|d| d.span.clone()).unwrap_or(0..0)` in `check_module` and passing it down.

- [ ] **Step 4: Thread a glyph-key span channel into `analyze`**

In `apps/emet/src/eval.rs`, have `run_module` also return `Vec<(String /*key*/, Span)>` per leaf (or a `HashMap<key, Span>` for the first occurrence AND the conflicting occurrence). The `Expr::Scroll`/glyph exprs carry spans; capture the glyph expr's span as each `Glyph` is built. Change `run_module`'s return type to `Result<(Vec<Scroll>, GlyphSpans), EvalError>` where `GlyphSpans` is an emet-side `Vec<...>` keyed to match `analyze`'s walk order.

In `apps/emet/src/lib.rs`, change `analyze(&self, scrolls, glyph_spans)` to look up the conflicting key's span:

```rust
pub fn analyze(scrolls: &[Scroll], glyph_spans: &std::collections::HashMap<String, std::ops::Range<usize>>) -> Result<(), Error> {
    use std::collections::HashMap;
    for scroll in scrolls {
        for unit in scroll.leaf_units() {
            let mut seen: HashMap<String, &Glyph> = HashMap::new();
            for r in unit.glyphs {
                let k = r.key();
                if let Some(prev) = seen.get(&k) {
                    if *prev != r {
                        let span = glyph_spans.get(&k).cloned().unwrap_or(0..0);
                        return Err(Error {
                            phase: Phase::Analyze,
                            msg: format!("two glyphs in this scroll both manage `{k}` with different contents — a leaf scroll can define each key only once"),
                            span,
                            note: None,
                        });
                    }
                } else {
                    seen.insert(k, r);
                }
            }
        }
    }
    Ok(())
}
```

Update `compile_all` and `resolve.rs`'s `analyze` callers to pass the span map and handle the new `Error` return. If building the exact conflicting-glyph span map cleanly requires touching more than `ir.rs`/`eval.rs`/`lib.rs`, fall back: key the map by glyph key to *any* occurrence span (still off `0..0`), and record the "underline the *second* glyph precisely" refinement as deferred to ADR 0032 in a `docs/TODO.md` line.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p emet --test scrolls --test diagnostics`
Expected: PASS. `assert_anchored_away_from_module_start` helper tests still hold.

- [ ] **Step 6: Run the whole crate**

Run: `cargo test -p emet`
Expected: PASS. `files.rs` (273/291) and `scrolls.rs` (83), `recursive_scroll.rs` (205) all assert the conflict *key* substring (`file:/etc/motd` etc.) — verify the new message still contains the key. The new message uses the key `k` inline, so `file:/etc/motd` survives. If any of those pins the OLD phrase `conflicting declarations for`, update it to the new substring and note it here.

- [ ] **Step 7: Commit**

```bash
git add apps/emet/src/eval.rs apps/emet/src/lib.rs apps/emet/src/infer.rs apps/emet/src/resolve.rs apps/emet/tests/scrolls.rs apps/emet/tests/diagnostics.rs
git commit -m "feat(emet): locate main-type and conflicting-key errors at their source spans

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** document the emet-side glyph-span channel (why the wire `Glyph`/`Scroll` stay span-free — versioned contract; the `main_span` threading; what precise-second-glyph underlining is deferred to ADR 0032).

**Locked-assertion updates:** `files.rs` 273/291 (`file:/etc/motd`, `file:/srv/x`), `scrolls.rs` 83 (`file:/etc/motd`), `recursive_scroll.rs` 205 (`file:/etc/x`) — all assert the *key* substring, preserved by inlining `k`. If any assert the old `conflicting declarations for` phrase, update to match the new message and list it here. Two assertions added.

---

## Task 15: Promote selected audit corpus cases into a permanent regression suite

Turn the audit into a durable gate. Create `apps/emet/tests/diagnostics_corpus.rs` that compiles selected corpus programs and asserts the *key phrase* of each rewritten message. Pin the cases worth locking (the correctness bugs, the leak fixes, the new detections) — not all 64.

**Files:**
- Create: `apps/emet/tests/diagnostics_corpus.rs`
- Reference (read-only): `.superpowers/sdd/errmsg/corpus/*.emet`
- Test: itself

**Interfaces:**
- Consumes: every message change from Tasks 1–14.
- Produces: a standing regression suite.

**Which cases to pin (the high-value subset — ~18 of 64):**
- Correctness bugs closed: 08 (bad escape → lex error), 22 (angle-bracket name rejected), 61 (dup binding), 16 (reserved-word binding), 19 (braced rollback).
- Leak fixes: 34 (no `t{n}` in arity), 38 (no `t{n}` in occurs).
- Constraint wording: 31 (`number`, no `satisfy`), 46 (`if` condition Bool), 41 (`policy` Policy).
- did-you-mean: 36 (`greeting`), 57 (`Nothing`), 58 (`String`).
- Syntax hints: 11 (empty `case` no arms), 12 (`=>`→`->`).
- Span/threading: 50 (conflicting key, non-zero span), 26 (let-in-arm rejected).
- Retry: 21 (valid-field list).

- [ ] **Step 1: Write the corpus regression suite (all assertions, red first)**

Create `apps/emet/tests/diagnostics_corpus.rs`. Read each corpus program from disk relative to the crate manifest and assert. The corpus lives at repo-root `.superpowers/sdd/errmsg/corpus`; from `apps/emet` that is `../../.superpowers/sdd/errmsg/corpus`.

```rust
use std::path::PathBuf;

fn corpus(name: &str) -> String {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../.superpowers/sdd/errmsg/corpus");
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
    let (_phase, msg) = err_msg("22-syntax-angle-bracket-name.emet");
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
    assert!(msg.contains("no arms") || msg.contains("at least one"), "{msg}");
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
    assert!(e.msg.to_lowercase().contains("let") || e.msg.to_lowercase().contains("not yet"), "{}", e.msg);
}
```

For `50-analyze-conflicting-keys` and `61-dup-binding`, add the span-and-phase assertions matching Tasks 14 and 10. Only include a case's test after its owning task has landed.

- [ ] **Step 2: Run the suite**

Run: `cargo test -p emet --test diagnostics_corpus`
Expected: PASS (all owning tasks 1–14 are already done when this task runs). If a case fails, the owning task's message drifted — fix the source, not the corpus test.

- [ ] **Step 3: Run the whole crate**

Run: `cargo test -p emet`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/emet/tests/diagnostics_corpus.rs
git commit -m "test(emet): promote the audit corpus into a permanent diagnostics regression suite

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** add a header comment (documenter) explaining the corpus path, the key-phrase (not full-message) assertion style, and that this file is the standing gate for AUDIT.md's rewrites.

**Locked-assertion updates:** none (new file). This suite is additive and pins the ~18 high-value cases.

---

## Self-Review

**Spec coverage (AUDIT plan sections):**
- Section (1): filename strip → Task 2; friendly typevars → Task 1; cleaned expected-sets → Task 3; plain constraint + reversed #41/#46 → Task 8; unclosed-delimiter → Task 4. ✓
- Section (2): reserved-word check → Task 6; did-you-mean → Task 9; angle-bracket/empty name → Task 12; bad string escape → Task 5; duplicate binding → Task 10; targeted syntax hints (#12/#13/#25/#23/#24/#29/#04/#11) → Task 7; both-vs-neither #18 (trivial) → Task 13; retry valid-fields #21 → Task 13. ✓
- Section (3) item 14 (span threading) → Task 14 (final, scope-honest split). Item 15 (two specific "not yet supported" parse errors) → Task 11. ✓
- OUT (general parse recovery, ADR 0032) — referenced, not built, in Tasks 4/11/14. ✓
- Corpus promotion gate → Task 15. ✓

**Placeholder scan:** no TBD/"add error handling"/"similar to Task N". Every code step shows complete code. Task 7's #13/#25/#23/#24/#29 hints are conservative (verified against corpus in Task 15); Tasks 11 (#27) and 14 (conflict-span precision) carry explicit honest-scope fallbacks with ADR 0032 references rather than placeholders.

**Type consistency:** `render_type` (Task 1) reused by Tasks 8, 9, 14. `humanize_expected` (Task 3) extended by Tasks 4, 7. `EvalError.span` added in Task 12, consumed in Task 14. `did_you_mean`/`edit_distance` defined once in Task 9. `is_reserved_constructor` reused in Task 6. `analyze` signature change (Task 14) updates all callers.

**Ordering:** renderers first (1–3), delimiter wording (4), then lexer (5) → parser detections (6,7,10,11) → infer (8,9,13) → eval/span-threading (12,14) → corpus gate (15). Tasks 12 and 14 are adjacent because 14 consumes 12's `EvalError.span`.
