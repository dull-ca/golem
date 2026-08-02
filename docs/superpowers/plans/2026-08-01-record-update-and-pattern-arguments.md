# Record Update and Pattern Arguments Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Emet gains Elm's record update (`{ r | f = v }`) and constructor patterns in argument position (`f (Box spec) = …`), then the LimeSurvey example collapses onto them and the language reference documents both.

**Architecture:** Both are additive front-end features over machinery ADR 0010 already built. Record update is one AST node, one inference rule over existing row unification, one eval case. Pattern arguments reuse the existing `Pattern` type in a new binding position, restricted to single-constructor types so `case` exhaustiveness (ADR 0005) is never bypassed.

**Tech Stack:** Rust (the `emet` crate: `lexer.rs`, `parser.rs` (chumsky), `ast.rs`, `infer.rs` (Algorithm W + rows), `eval.rs`), Emet source for the example, Astro/Starlight MDX for the reference.

## Global Constraints

- Governing decision: `docs/adr/0044-record-update-and-pattern-arguments.md`. Syntax and semantics are **Elm's**; do not invent variants. Where the ADR is silent, follow Elm.
- `lw:implementer` writes ZERO comments; `lw:documenter` owns every comment and doc afterward.
- Existing behaviour is untouched: every current `.emet` in `examples/`, `lib/`, `apps/fleet/`, and the compiler's own fixtures must still compile to **byte-identical** manifests. Diff against a `git archive HEAD` extraction.
- `nix flake check` is the CI gate (ADR 0035); `cargo test --workspace` must stay green, including `apps/emet/tests/docs_examples.rs` (ADR 0043).
- Errors are Elm-style and actionable (ADR 0032): naming the offending field/constructor and, where useful, listing what was available.

---

### Task 1: Record update — `{ r | field = value, … }`

**Files:**
- Modify: `apps/emet/src/ast.rs` (new `Expr` variant), `apps/emet/src/parser.rs` (record-literal path), `apps/emet/src/infer.rs` (inference rule), `apps/emet/src/eval.rs` (evaluation)
- Test: `apps/emet/tests/pipeline.rs` (or a new `apps/emet/tests/record_update.rs` if that file is already large)

**Interfaces:**
- Produces: `Expr::RecordUpdate { base: Box<Expr>, fields: Vec<(String, Expr)>, span }`. Later tasks and the LSP read this name.
- Parsing: inside `{ … }`, after parsing an expression, a `|` means update; otherwise it is a record literal. The base is an arbitrary expression, as in Elm.
- Typing: unify the base against an open record demanding each updated field; the result is the base's type with those fields' types substituted. Updating an absent field is a type error naming the field.
- Evaluation: shallow copy of the base record with the named fields replaced. Field expressions evaluate left to right.

- [ ] **Step 1: Write failing tests.** At minimum: a literal base (`{ { a = 1, b = 2 } | a = 9 }`); a variable base; multiple fields at once; the updated value changing type where rows allow it; a field the record lacks → type error naming it; a non-record base → type error. Include one test asserting the error *message*, not just that it errored.
- [ ] **Step 2: Run them; confirm they fail for the stated reason** (parse error today, not a wrong-value pass).
- [ ] **Step 3: Implement** across ast/parser/infer/eval.
- [ ] **Step 4: Run the new tests, then the full `cargo test -p emet`.**
- [ ] **Step 5: Prove no regression** — every repo `.emet` still builds and manifests are byte-identical to `git archive HEAD`.
- [ ] **Step 6: Commit** (`lw:historian`).

### Task 2: Constructor patterns in argument position

**Files:**
- Modify: `apps/emet/src/parser.rs` (function/lambda parameter parsing), `apps/emet/src/infer.rs` (binding a pattern in an argument), `apps/emet/src/eval.rs` (destructuring on application)
- Test: same suite as Task 1

**Interfaces:**
- Consumes: nothing from Task 1; the two are independent and may land in either order.
- Surface: `f (Box spec) = …` for top-level declarations and `\(Box spec) -> …` for lambdas.
- Restriction: **single-constructor types only.** A pattern over a multi-constructor type is a compile error directing the author to `case`, so exhaustiveness (ADR 0005) cannot be bypassed. Nested patterns are out of scope for this task; a single constructor binding one name is the target.

- [ ] **Step 1: Write failing tests.** A single-constructor destructure in a top-level decl; the same in a lambda; a multi-constructor type → error naming the type and pointing at `case`; arity mismatch → error. Assert one error message.
- [ ] **Step 2: Run them; confirm they fail as parse errors today.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: `cargo test -p emet` green.**
- [ ] **Step 5: Byte-identical manifest check, as Task 1.**
- [ ] **Step 6: Commit.**

### Task 3: Collapse the LimeSurvey example onto the new syntax

**Files:**
- Modify: `examples/limesurvey/Limesurvey.emet`, `examples/limesurvey/main.emet`, `examples/limesurvey/Ingress.emet` (only if it benefits)

**Interfaces:**
- Consumes: Tasks 1 and 2.
- The example currently carries `withAdministrator` and `withAdministratorEmail`, each rebuilding all five `Config` fields after a `case` unwrap. With both features they become one line each — or disappear entirely if exposing `Config(..)` plus a `defaults` value reads better, which ADR 0044's consequences argue it does. **Decide which, and say why in the report.**
- The example's behaviour must not change: the built manifest for `examples/limesurvey/main.emet` must be byte-identical before and after. That is the proof this is a pure simplification.

- [ ] **Step 1: Capture the current manifest** (`cargo run -q -p emet -- build examples/limesurvey/main.emet -o /tmp/before.bin`).
- [ ] **Step 2: Rewrite the config surface** using record update and pattern arguments.
- [ ] **Step 3: Rebuild and `cmp` against `/tmp/before.bin`** — must be identical.
- [ ] **Step 4: `cargo test -p emet` green** (the example is guarded by `repo_examples_compile.rs`).
- [ ] **Step 5: Commit.**

### Task 4: Document both features

**Files:**
- Modify: `sites/website/src/content/docs/reference/language/values-and-types.mdx` (record update), `sites/website/src/content/docs/reference/language/pattern-matching.mdx` (argument patterns), `sites/website/src/content/docs/reference/status.mdx` (both now implemented)
- Modify: `apps/emet/CLAUDE.md` (the language surface summary it maintains)
- Modify: `docs/adr/0044-record-update-and-pattern-arguments.md` (Proposed → Accepted, with the implementing commits)

**Interfaces:**
- Consumes: Tasks 1–3.
- Reference pages are information-oriented: state the syntax, the typing rule, and the restriction; no advocacy. Follow the existing language pages, which the site's own Diátaxis review rated its cleanest reference.
- Where a doc example is a complete program, prefer moving it into `sites/website/examples/` under the ADR 0043 harness so CI compiles it, rather than adding another literal fence.

- [ ] **Step 1: Update the two language reference pages and `status.mdx`.**
- [ ] **Step 2: Update `apps/emet/CLAUDE.md`.**
- [ ] **Step 3: Flip ADR 0044 to Accepted, naming the commits.**
- [ ] **Step 4: `cd sites/website && bun run build` succeeds; `cargo test --workspace` green.**
- [ ] **Step 5: Commit.**

## Self-review notes

- Spec coverage: ADR 0044's two decisions are Tasks 1 and 2; its "consequences" claim about dropping `with*` families is exercised by Task 3; the explicitly-rejected alternatives (Scala `.copy`, Rust `..`, opaque-wrapper update) are out of scope and must stay out.
- Type consistency: `Expr::RecordUpdate` is the only new AST name crossing task boundaries; Task 2 adds no shared type.
- The byte-identical manifest check appears in Tasks 1, 2 and 3 deliberately — it is the single cheapest proof that a front-end change altered no output.
