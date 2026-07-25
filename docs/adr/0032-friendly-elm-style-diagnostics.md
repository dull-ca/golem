# 0032-friendly-elm-style-diagnostics

## Status

Proposed 2026-07-25.

Builds on ADR 0022 (one report per error; parse-phase recovery at the layout `;`
boundary) and reaffirms its one-clean-error rule as a diagnostic principle rather
than only a plumbing decision. Extends ADR 0026's precedent — a specific,
redirecting compile error in place of a bare parse failure — to the whole
diagnostic surface. Closes the ADR 0031 §4 / `docs/TODO.md` scroll-name
follow-up. Touches no wire format: diagnostics are `emetc`-side only, no
`format_version` change.

## Context

A full audit of every diagnostic `emetc build` emits — 64 corpus programs graded
against Elm's "compiler errors for humans" principles — is recorded at
`.superpowers/sdd/errmsg/AUDIT.md`. Half the corpus grades C or D (A=16, B=16,
C=15, D=17), and three findings are outright correctness bugs, not wording:

- **An angle-bracket scroll name compiles clean.** `scroll { name = "<removes>" }`
  exits 0 and emits a manifest (#22), colliding with golemd's synthetic
  `<removes>` reporting segment (ADR 0031 §4, `docs/TODO.md` known gap). A silent
  wrong-accept is the worst possible diagnostic.
- **A bad string escape is swallowed** (#8). `"\q"` produces no lex or parse
  error; the escape is silently dropped and the author sees an unrelated `main`
  type error. The char-escape path already validates; the string path does not.
- **A duplicate binding is silently ignored** (#61). `x = 1` then `x = 2` reports
  nothing about the redefinition; the second binding is dropped.

The remaining C/D grades share recurring defects: internal type-variable names
leak into messages (`t9 -> t10`, `t1 occurs in t1 -> t2`); the compiler's
constraint vocabulary surfaces raw (`String does not satisfy 'number'`); chumsky's
`expected` lists leak virtual layout tokens and the placeholder `something else`;
the entry file's path is embedded in every parse-error label, duplicating the
report header and wrecking layout; analyze- and `main`-type errors pin to `1:1`
because the IR drops source spans (#50, and #8/#26/#27/#61 collapsing to the
`main` decl); reserved constructor words bound as ordinary names produce a
type-variable leak instead of naming the word (#16, #19) — the "definable but
unusable" trap; and unbound names / unknown constructors offer no did-you-mean
even when a near-match is in scope (#36, #37, #57, #58, #63).

Dr. Dub's directive: Emet's diagnostics should be **friendly and helpful, like
Elm's**. This ADR records the contract that directive settles and scopes the
work; the per-finding rewrites, fix classes, and prioritized plan live in the
audit — this is the decision, not the worklog.

## Decision

**Adopt Elm's error-message principles as Emet's diagnostic contract, add the
detections the contract requires (several of which close correctness bugs), and
thread source spans through eval → analyze so a diagnostic points at the offending
expression.**

### 1. The diagnostic contract

Every diagnostic `emetc` emits meets these, and the audit corpus is the bar:

- **Precisely located, with a source excerpt.** One ariadne report with the span
  on the thing that is wrong — for an unclosed delimiter, the *opener*, not the
  token that follows it (#01/#02/#03).
- **Plain language.** State what the compiler was doing and what it expected. No
  internal vocabulary: no `t31`-style type variables (render unbound variables as
  `a`, `b`, …), no raw constraint names (`does not satisfy 'number'` becomes "works
  on numbers, but this is a String"), no `unification`.
- **Full expected-vs-found types**, with the direction correct (the field wanting a
  `Policy` is expected, the given `String` is found — not reversed, #41/#46).
- **No chumsky noise.** Strip the filename from parse-error labels (it is already
  in the header), dedupe the `expected` set, drop virtual layout tokens
  (`VSemi`/`VRBrace`) from user-facing lists, and replace `something else` with a
  concrete word.
- **A concrete hint or likely fix where one is inferable**, including did-you-mean
  by edit distance over names in scope.
- **A friendly, non-blaming tone.**
- **One clean error per report** — reaffirming ADR 0022. The existing
  one-report-per-error rule is now a stated principle, not just the shape the
  `Vec<Error>` surface happened to take.

### 2. New detections — part of the language surface

Each closes a specific audit finding; the ones marked *(bug)* close a silent
miscompile or wrong-accept.

- **(a) Reserved-word binding / bare use.** Binding or bare-valuing a reserved
  constructor word — `keep n = …`, `policy = rollback { }`, `file "x"` — is a
  parse-time error that names the word as reserved and shows the correct shape
  (`rollback` is braceless: `policy = rollback`; `keep 3` builds without braces;
  `file { path, contents, mode }`). Kills the "definable but unusable" trap and
  the `t9 -> t10` / `{ } -> t2` leaks (#16, #19). The reserved set is
  parser-level, not lexer-level: `is_reserved_constructor` (`parser.rs`) is a
  predicate the `var` / `constructor_name` / `policy_word` selects branch on a
  plain `Tok::Ident` — the lexer does not mint keyword tokens. The new detection
  extends the same predicate to binding heads and bare-value position; it does
  not move reservation into the lexer.
- **(b) Scroll-name validation** *(bug, #22)*. A scroll `name` must be a
  **non-empty, printable host identifier containing no angle brackets** (`<` or
  `>`). An empty name, or one containing `<`/`>`, is a compile error at
  build/eval time. This closes the `<removes>` forgeability gap (ADR 0031 §4,
  `docs/TODO.md`): the synthetic removes segment is now unforgeable because no
  authored scroll can be named with angle brackets. The rule is deliberately
  minimal — reject the empty string and the two bracket characters — rather than a
  full hostname grammar, which would reject legitimate names golemd already
  accepts.
- **(c) Invalid string escape → lex error** *(bug, #8)*. An unrecognized string
  escape (`"\q"`) is a lex error naming the escape and listing the valid ones,
  mirroring the char-escape validation that already exists (`lexer.rs`).
- **(d) Duplicate binding → error** *(bug, #61)*. A name bound twice at the top
  level, or twice in one `let`, is a compile error underlining the second
  binding — no more silent drop.
- **(e) Did-you-mean suggestions** by edit distance for unbound names, unknown
  constructors, unknown type constructors, and unknown record / retry fields
  (#36, #37, #57, #58, #63) — over names in scope, and for a qualified name over
  the module's exposed set.
- **(f) Targeted syntax hints**: `=>` → `->`, missing `of` / `in` / `=`, an empty
  `case`, a lambda's missing `->`, and an unclosed delimiter pointing at the
  opener (#04, #11, #12, #13, #23, #24, #25, #29).

### 3. Spans thread through eval → analyze

The IR drops source spans today, so `analyze` (`lib.rs`) sets `0..0` and a
`main`-type error pins to the `main` declaration — the root cause of several
mislocations (#50 conflicting keys at `1:1`; #8/#26/#27/#61 collapsing to `main`).
Thread each glyph's and expression's source span through eval into analyze so an
analysis or `main`-type error underlines the offending glyph/expression, not
`1:1`.

### 4. Deferred, and why

Recorded here so the boundary is deliberate, not forgotten:

- **General parse-error recovery / resynchronization** beyond ADR 0022's
  `VSemi`-boundary sync (a recovery pass at arm/declaration boundaries that stops
  a single typo cascading, #12). Staged later: the per-detection recovery rules in
  §2(f) cover the real cascade cases first, at far lower risk than a general
  resync pass.
- **`let … in` inside a `case` arm** (#26) and **a negative literal as a function
  argument** (#27), both `docs/TODO.md` known gaps. Real support is
  language-design scope. Until then each gets a **specific "not yet supported
  here" parse error** naming the unsupported form — strictly better than today's
  misleading `main must be List Scroll but is Int`.

## Alternatives considered

1. **Leave the diagnostics as they are.** Rejected: half the corpus grades C/D
   and three findings are correctness bugs (a wrong-accept and two silent
   miscompiles), not cosmetics. "Friendly and helpful" is a stated product
   requirement, and the current messages leak compiler internals the audit
   catalogs one by one.
2. **Multi-error reporting like modern compilers** (accumulate and render many
   type errors per run). Rejected for now: it conflicts with the one-clean-error
   rule ADR 0022 settled, and multi-*type*-error recovery means fabricating types
   for un-inferable holes, risking spurious cascades and corrupted exhaustiveness
   results (ADR 0022's parse-vs-type boundary). Parse recovery already reports
   several *parse* errors; later phases stay first-error by design.
3. **Move policy/glyph words into lexer-reserved keywords** rather than the
   parser-level predicate. Rejected: it does not match the implementation — the
   lexer emits a plain `Tok::Ident` and `is_reserved_constructor` (`parser.rs`)
   decides at parse time. Lexer reservation would also make every glyph word a
   hard keyword everywhere, breaking their dual build/match role (ADR 0017). The
   §2(a) check extends the existing parser predicate instead.

## Consequences

- **~23 existing test assertions on message text will be updated.** The audit
  lists them (`apps/emet/tests/diagnostics.rs`, `files.rs`, `scrolls.rs`,
  `modules.rs`, the pattern-matching suites, and more) — every
  `e.msg.contains(...)` that locks current wording changes when the message does.
  These are edits to existing assertions, expected and enumerated.
- **The new detections are breaking for programs that today compile.** A program
  that bound a reserved word (`keep`, `file`, …) as a name, relied on a swallowed
  bad string escape, shipped a duplicate binding, or used an angle-bracket scroll
  name compiles today and will be a compile error after. This is intended: two of
  those are silent miscompiles and one a wrong-accept. New behavior ships with its
  own tests rather than editing the wording-lock assertions above.
- **The audit corpus becomes a regression suite.** The 64 programs under
  `corpus/` with their captured output are the fixture that holds the contract:
  each rewrite lands against its corpus case, and the grade distribution is the
  measure of progress.
- **Threading spans through the IR touches eval and analyze** (§3). The IR
  carrying spans is a small structural change to inert data; it is the root-cause
  fix several mislocations share, so it is done once rather than patched per site.
- **Scoped to `emetc`.** No `golemd`, no `scroll-format`, no `format_version`
  change — diagnostics never cross the wire.
- **What this forecloses:** an author can no longer name a scroll with angle
  brackets (§2b), so the golemd `<removes>` convention (ADR 0031 §4) is now
  enforced, not conventional; and reserved constructor words are permanently
  unbindable as ordinary names (§2a), closing the definable-but-unusable trap for
  good.

## Cross-references

- ADR 0022 — one report per error; parse-phase recovery (reaffirmed and elevated
  to a principle; the type-phase first-error boundary is why multi-type-error
  reporting is rejected).
- ADR 0026 — a specific, redirecting compile error in place of a bare parse
  failure (the Float-pattern precedent this generalizes).
- ADR 0031 §4 — the `<removes>` reporting segment whose forgeability §2(b) closes.
- ADR 0017 — the build/match constructor split (why glyph/policy words stay
  parser-level `Tok::Ident`, not lexer keywords).
- `.superpowers/sdd/errmsg/AUDIT.md` — the per-finding grades, rewrites, fix
  classes, prioritized plan, and the test-assertion list.
- `docs/TODO.md` — the scroll-name, `let…in`-in-arm, and negative-argument known
  gaps this ADR schedules or resolves.
