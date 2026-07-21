# 0005-custom-types-case-of-and-exhaustiveness

## Status

Proposed

## Context

To model Elm, Emet needs sum types (`Maybe a = Just a | Nothing`,
`Bool = True | False`) and `case … of` pattern matching, plus `if … then …
else …`. Today there are no sum types, no constructors-as-values, and no
elimination form at all — `Value` is `Str | Glyph | Glyphs | Record | Closure`.

Emet's **totality invariant** (guaranteed termination, finite glyph DAG, no
general recursion) makes the elimination form dangerous in one specific way: a
**non-exhaustive `case`** could reach a scrutinee with no matching arm — a
runtime "no match" failure. That is a totality violation. Any `case` design must
therefore make exhaustiveness a *compile-time* guarantee, not a runtime
fallthrough.

There is also a soundness interaction with ADR 0002: glyph subsumption
(`AptPackage`/`SystemdService` inject into `Glyph`) is sound **only because
glyphs have no elimination form**. Adding `case` must not create one for glyphs.

The `of`/`where` layout keywords are already reserved and already open layout
blocks (`layout.rs::opens_layout`); ADR 0001 explicitly warns that using
`case … of` requires attention to the layout close rules.

## Decision

**Add custom sum types via a `type` declaration, constructors as first-class
values, and `case … of` / `if` — with compile-time exhaustiveness and redundancy
checking to preserve totality.**

### Custom types

- Surface (exact Elm): `type Maybe a = Just a | Nothing`, `type Bool = True |
  False`. A `Module` gains `type_decls`, processed before value decls.
- Each variant contributes a **value constructor** as a polymorphic scheme in
  the type env: `Just : ∀a. a -> Maybe a`, `Nothing : ∀a. Maybe a`,
  `True/False : Bool`. Constructors are first-class values (so `List.map Just`
  works).
- `Maybe` and `Bool` are defined through this mechanism (injected
  programmatically first, optionally migrated to a prelude source later) — they
  are **not** hardcoded into `unify`. `List` keeps literal syntax + builtin
  support, as in Elm.

### Values

`Value` gains `Data { ctor: String, args: Vec<Value> }`; a saturated constructor
is `Data`, an unsaturated one is an arity-collecting closure. `Value::Glyphs`
generalizes to `Value::List(Vec<Value>)`.

### `case` / `if`

- `case scrut of` opens a layout block after `of` (already handled). Arms are
  separated by the same-column `VSemi` rule and closed by **dedent** — `case`
  has **no `in`**, so this needs **no new `parse-error(t)` rule** (the easy half
  of the ADR-0001 caveat). Wave-with-case requires **laid-out arms** (each arm on
  its own line, or explicit `{ … ; … }`); one-line inline `case` is deferred
  until we decide whether a `parse-error(t)`-style close is worth it.
- Patterns: `_` (wildcard), lowercase ident (var-bind), `Upper` + sub-patterns
  (constructor), string literal.
- `if c then t else e` is **desugared to** `case c of True -> t ; False -> e`, so
  there is one elimination code path.
- Inference: a constructor pattern instantiates its constructor scheme, unifies
  its result with the scrutinee type, and unifies field types with sub-patterns;
  a var pattern binds `x : scrutineeType`; `_` binds nothing; a string-literal
  pattern unifies the scrutinee with `String`. All arm bodies unify to one type.

### Exhaustiveness + redundancy (totality-critical, same wave)

- **Exhaustiveness is a compile error, never a runtime fallthrough.** For a
  scrutinee of sum type `T`, the arms must cover every constructor (a var/`_` arm
  covers the remainder); missing constructors → error listing them. For a
  `String` scrutinee (infinite domain) a catch-all is required.
- **Redundancy** (unreachable arm — e.g. an arm after a catch-all, or a repeated
  constructor) → error.
- Recommended algorithm: the standard Maranget (2007) *usefulness* check
  restricted to Emet's small pattern language — it yields exhaustiveness *and*
  redundancy together and handles nested patterns; a shallow top-level-only check
  is an acceptable time-boxed fallback.
- `eval` for `case` may `unreachable!()` on no-match, because the checker
  guarantees a match exists (mirroring the existing impossible-by-typing
  `unreachable!` uses in `eval.rs`).

### Glyphs stay non-matchable *for now* (matching is planned, deferred)

Glyph pattern-matching is **not forbidden in principle** — it is deferred.
Matching a glyph would be a trivial `case`; the interim hazard is **not**
matching itself but ADR 0002's **symmetric permissive-injection** shortcut, which
is sound only while nothing eliminates a glyph. So, near term: there are **no
glyph constructors/patterns** yet, and until one exists the symmetric injection
stays sound and constructors keep returning their **precise subtype**
(`aptPackage … : AptPackage`). When glyph `case` is wanted, the symmetric arm
must first be replaced by a principled model (directed nominal subsumption, or
row/variant typing) — see ADR 0008 and the design doc §5. The Wave-0
`Con`-head + bounded-`Var` foundation is chosen so that replacement is
**additive, not a repaint**. This `case`/exhaustiveness design already provides
the elimination machinery a future glyph model would reuse.

## Alternatives considered

1. **Hardcode `Maybe`/`Bool` in the compiler** instead of a general `type`
   mechanism. Rejected: less Elm-faithful and, long-term, *more* special-casing;
   the `type` mechanism makes them ordinary declarations.
2. **Runtime `_`-fallthrough to a crash / error value on non-exhaustive `case`.**
   Rejected: breaks totality; the whole point is a compile-time guarantee.
3. **Allow one-line `case` now** (add a `parse-error(t)`-style close rule).
   Deferred: pays ADR-0001's layout debt for a minor ergonomic; laid-out arms
   ship first.
4. **Skip redundancy checking.** Rejected: the usefulness algorithm yields it for
   free and it catches real mistakes; strictness suits a total language.

## Consequences

- Emet gains sum types, constructors, `case`, and `if` while remaining total —
  branching is added, looping is not.
- Exhaustiveness/redundancy checking is a new, self-contained compile phase; it
  is the mechanism that protects totality against the new elimination form.
- One documented syntactic divergence from Elm: laid-out `case` arms (no one-line
  `case`) initially.
- `if` has a single implementation via desugaring to `case`.
- Glyphs stay non-matchable *for now* — deferred, not foreclosed; the
  `case`/exhaustiveness machinery here is exactly what a future glyph model
  (ADR 0008) reuses.
- Cross-references ADR 0001 (layout / `parse-error(t)`), ADR 0002 (interim glyph
  subsumption), ADR 0008 (deferred glyph matching), and the design doc
  `docs/design/0001-…` §5–§8.

## Addendum — list patterns

The exhaustiveness/redundancy checker now covers lists. `List` is modeled as a
two-constructor sum `{ [], :: }`: `[]` and `head :: tail` patterns lower to those
synthetic constructors (`prelude::NIL`/`CONS`, with schemes `∀a. List a` and
`∀a. a -> List a -> List a`), and `prelude::sum_type_constructors("List")`
reports both as the complete signature. A `case` on a list is therefore
exhaustive exactly when it covers both cases, and a redundant list arm is caught
like any other. The Maranget algorithm itself is unchanged — only the synthetic
constructors and their schemes were added, no list-specific code path.
