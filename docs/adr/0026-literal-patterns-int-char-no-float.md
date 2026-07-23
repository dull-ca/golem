# 0026-literal-patterns-int-char-no-float

## Status

Accepted — approved and implemented; the full suite is green (386 tests).

**As built**, one mechanism deviates from §3: the Float rejection uses chumsky's
`.validate` with a non-fatal `emitter.emit`, not `try_map` / `Rich::custom`. The
pattern atom sits inside a `repeated()` over the `case` arms, and `repeated()`
rewinds and swallows a hard parse failure — so the error must be emitted
non-fatally to survive, with an inert `Pattern::Wildcard` returned as a
placeholder that never reaches inference. Same span, same message, same `Parse`
phase; only the emission mechanism changed.

## Context

A `case` in Emet may match exactly one kind of literal: a `String`.
`ast::Pattern` is `Wildcard | Var | Str | Ctor | Nil | Cons` — there is no
`Int`, `Float`, or `Char` literal pattern (`ast.rs`). Yet the *expression* side
already lexes and parses all three: `Tok::Int`/`Tok::Float`/`Tok::Char` exist
(`lexer.rs`), and `expr_parser` has `int_lit`/`float_lit`/`char_lit` combinators
(`parser.rs`, ~ll. 303–312) that build `Expr::Int`/`Float`/`Char`. So
`case c of 'a' -> …` is a parse failure today only because `pattern_parser`'s
`atom` omits those alternatives; the token support is already there.

Dr. Dub's decision: add literal patterns **faithfully to Elm**.

- Elm allows `Int` literal patterns (`case n of 0 -> …`) and `Char` literal
  patterns (`case c of 'a' -> …`).
- Elm **rejects `Float` literal patterns** — IEEE-754 equality is unreliable, so
  it refuses to compile a float pattern and steers the author to `<`/`>` in an
  `if`. We replicate the rejection, with a helpful compile error, rather than
  admit a `Pattern::Float` that silently matches by `==`.
- `String` patterns already work and are unchanged.

Ground truth confirmed against the current tree:

- **Typing (`infer::infer_pattern`).** `Pattern::Str(_)` unifies the scrutinee
  with `con("String")` (`infer.rs:1153`). Integer literal *expressions* are
  typed as a fresh `number` variable defaulting to `Int` (`Constraint::Number`),
  and `Expr::Char` types as `con("Char")` — the same `con("Char")` a char
  pattern will want.
- **Exhaustiveness (`infer.rs`, Maranget usefulness).** The lowered pattern
  `UPat` is `Wild | Ctor | Str`; the head key `Head` is `Ctor | Str`.
  `complete_signature` returns a finite constructor set only for a `Con` sum
  type and `None` otherwise, and a `Head::Str` never participates in a
  signature — so a `Str` column is an **open/infinite domain**: a wildcard is
  always still useful over it, i.e. a catch-all `_`/binder is mandatory, and a
  duplicate string literal is redundant. There is **no** `Str`-specific
  completeness code; strings ride the generic open-domain path
  (`specialize`/`default_matrix`/`useful`). Int and Char must slot into exactly
  this path.
- **Eval (`eval::match_pattern`).** `Pattern::Str(s) => matches!(value,
  Value::Str(v) if v == s)` (`eval.rs:233`). `Value::Int(i64)` and
  `Value::Char(char)` already exist (`eval.rs`), so an Int/Char matcher arm is a
  direct equality check with no new runtime representation.
- **The unary-minus lexer quirk.** A leading `-` is **never** part of a numeric
  token: `-1` lexes as `Tok::Op("-")` then `Tok::Int(1)` (the number scanner
  starts only at `is_ascii_digit`, `lexer.rs:562`; `-` is an operator char,
  `lexer.rs:147`). In expression position the `unary` layer folds a leading `-`
  into `negate` application (`parser.rs:409`). Pattern position has no such
  fold, so `case n of -1 -> …` is a parse error today. Elm accepts it; this ADR
  must decide how.
- **One report per error (ADR 0022).** Parser errors are `Rich<Tok, TokSpan>`
  with a span, rendered by ariadne. A `try_map` returning `Rich::custom(span,
  msg)` is the established way to raise a semantic parse error with a precise
  span (see `build_constructor`, `parser.rs:903`).

## Decision

### 1. Two new pattern forms; `Str` unchanged

Add to `ast::Pattern`:

```rust
/// An integer literal — matches an equal integer. Typed `number` (default
/// `Int`), mirroring integer literal expressions, not hard-`Int`.
Int(i64),
/// A char literal `'c'` — matches an equal `Char`.
Char(char),
```

`Pattern::Str(String)` is untouched. There is **no** `Pattern::Float` — see §3.

### 2. Parser: reuse the existing literal token selects as pattern atoms

`pattern_parser`'s `atom` becomes:

```text
atom = wildcard | var | str_lit | char_lit | int_lit
     | nullary_ctor | list_literal | paren
```

`int_lit` and `char_lit` are the same `select! { Tok::Int(n) => … }` /
`select! { Tok::Char(c) => … }` shape already used in `expr_parser`, building
`Pattern::Int`/`Pattern::Char` with the token's span. No lexer change.

### 3. Float literals in pattern position are a compile error (Elm-faithful)

A `Tok::Float` reaching pattern-atom position is rejected with a **dedicated
semantic diagnostic carrying the float's span** — not a generic "unexpected
token" parse error, because the entire point is the *helpful* message. Add a
pattern-atom alternative that matches `Tok::Float` and fails via `try_map` /
`Rich::custom(span, msg)` (one report per error, ADR 0022):

> **`Float` literals can't be matched in a pattern.** Floating-point equality is
> unreliable, so Emet — like Elm — forbids it. Bind the value with a name and
> compare it with `<`, `>`, `<=`, or `>=` in an `if` instead.

Catching `Tok::Float` explicitly (rather than letting it fall through to a bare
parse failure) is what guarantees the author sees this message with the literal
underlined.

### 4. Typing

In `infer::infer_pattern`:

- `Pattern::Char(_)` → `inf.unify(scrutinee, &con("Char"), &pat.1)`, mirroring
  `Pattern::Str`'s `con("String")`.
- `Pattern::Int(_)` → unify the scrutinee with a **fresh `number`-constrained
  variable** — `inf.fresh_constrained(Constraint::Number)` (the same mint an
  integer literal *expression* gets), **not** `con("Int")`. Matching an integer
  literal is a `number`, so it stays polymorphic between `Int` and `Float` and
  defaults to `Int` if nothing pins it — exactly as the expression side behaves,
  and exactly as Elm's `number` patterns behave.

Consequence to record: because an int pattern is `number`, matching an integer
literal against a `Float`-typed scrutinee **typechecks** at the `number` level
(the literal is a `Float`). This is Elm-consistent. It does not reopen the Float
question: a *`Float` literal* in pattern position is still rejected at parse time
(§3); only an *integer* literal against a float value is allowed, and that
matches by ordinary equality on the `Value::Float`/`Value::Int` the number
resolves to.

### 5. Negative integer literal patterns

Support `case n of -1 -> …` with the **minimal faithful** fold, mirroring the
expression `unary` layer. In pattern-atom position, a `Tok::Op("-")`
**immediately adjacent** to a following `Tok::Int(n)` (the minus's span end
equals the int's span start, the same adjacency test `qualified` already uses in
`parser.rs:321`) folds to `Pattern::Int(-n)`, spanning both tokens. A `-`
followed by a `Tok::Float` folds into the float-rejection of §3 (a negative
float is still a float). A `-` not adjacent to a numeric literal is a parse error
as before.

This is small and localized — one alternative in the pattern `atom`, no lexer
change — and keeps Elm parity, so it is **not** deferred.

### 6. Exhaustiveness and redundancy: no new completeness logic

Extend the lowered types with literal heads that behave **exactly like `Str`**:

- `UPat`: add `Int(i64)` and `Char(char)` beside `Str(String)`.
- `Head`: add `Int(i64)` and `Char(char)` beside `Str(String)`.
- `lower_pattern`: `Pattern::Int(n) => UPat::Int(n)`,
  `Pattern::Char(c) => UPat::Char(c)`.
- `head_of` / `specialize`: the new heads are arity-0 and match by value
  equality, the same shape as the `Str` arm.

Nothing else changes. `complete_signature` still returns `None` for a scrutinee
that isn't a `Con` sum type, so an `Int`/`Char`/`String` column is an open
domain: a `case` over literals is exhaustive **only** with a trailing `_` or
binder, and a duplicate literal (or any arm after a catch-all) is **redundant**.
The `missing_constructors` fallback message ("add a `_` catch-all arm …")
already covers the non-`Con` case. Confirmed against `infer.rs`: the usefulness
algorithm is agnostic to what an open literal head *is*, so Int and Char need no
literal-specific logic.

Subtlety recorded: `Char` and `Int` are technically *finite* domains
(2³² scalars, 2⁶⁴ integers), so an enumeration of every value would be
"complete". We treat them as **open**, like Elm — enumerating them is never
practical, and requiring a catch-all is the only sane rule. We deliberately do
not special-case a `case` that happens to list every `Char`.

### 7. Eval

Add two matcher arms to `eval::match_pattern`, direct equality like `Str`:

```rust
Pattern::Int(n)  => matches!(value, Value::Int(v)  if v == n),
Pattern::Char(c) => matches!(value, Value::Char(v) if v == c),
```

No new `Value` variant — `Value::Int`/`Value::Char` already exist. A negative
pattern matches because §5 folds it to `Pattern::Int(-n)`, a plain integer.

## Alternatives considered

- **Admit `Pattern::Float` matching by `==`.** Rejected: IEEE-754 equality makes
  it a footgun (`0.1 + 0.2` never matches `0.3`); Elm forbids it for this
  reason, and silent mismatches would violate the "no surprising runtime
  behavior" spirit even though `case` stays total.
- **Type int patterns as hard `Int`.** Rejected: it diverges from integer
  literal *expressions* (which are `number`) and would spuriously reject
  matching an integer literal against a `Float`-typed value. `number` keeps
  patterns and expressions symmetric.
- **Defer negative literal patterns.** Rejected: the fold is a few lines and
  matches an idiom (`-1`, `0`, `1`) authors will reach for; deferring it would
  be a visible, un-Elm-like gap. See §5.
- **Reject Float via a generic parse error.** Rejected: the helpful, redirecting
  message *is* the feature (§3); a bare "unexpected token" throws away the reason
  and the fix.

## Consequences

- `case` gains Int, Char, and (already) String literal matching, all sharing the
  one open-domain exhaustiveness path — no new completeness code, so the Maranget
  checker stays small.
- Literal `case`s require a trailing `_`/binder to be exhaustive, and duplicate
  literals are flagged redundant — consistent with how string patterns behave
  today.
- Float pattern matching is a compile error with a precise span and a redirect to
  comparison operators; authors branch on floats with `if x < … then …`.
- Negative integer patterns work via a localized parse-time fold; the lexer's
  unary-minus quirk is contained to the parser, not pushed into the lexer.
- Forecloses: no `Pattern::Float` variant enters the AST, so no downstream stage
  ever reasons about float equality; and Int/Char are fixed as **open** domains,
  so a future "exhaustive enumeration of every `Char`" is intentionally not a
  path to exhaustiveness.
- Implementation surface: `ast.rs` (two variants), `parser.rs` (pattern atoms +
  negative fold + Float rejection), `infer.rs` (two typing arms + `UPat`/`Head`
  arms + `lower_pattern`/`head_of`/`specialize`), `eval.rs` (two matcher arms).
  Tests: int/char/string match; exhaustiveness-requires-wildcard;
  redundant-duplicate-literal; the Float-pattern error message + span; a
  negative-int pattern (`-1`).

## Cross-references

- ADR 0005 — `case`, exhaustiveness/redundancy checking (the Maranget checker
  this extends).
- ADR 0007 — numbers as constrained `number` variables (why int patterns are
  `number`, not `Int`).
- ADR 0009 — pattern language / list patterns (the last additions to the pattern
  vocabulary).
- ADR 0017 — glyph pattern matching (the other consumer of the pattern language
  and the exhaustiveness checker).
- ADR 0022 — one report per error (how the Float rejection is rendered).
- ADR 0025 — the `Char` primitive type and `Value::Char` (the type these
  patterns match; char *patterns* were explicitly deferred there).
