# 0027-tuples-and-unit

## Status

Accepted — approved, implemented, and green.

**As built.** The implementation follows the decision below, with three
deviations worth recording:

- `collect_rigids` needed a `Tuple` arm too — an extra `Type`-walker beyond the
  list §3 enumerated. Without it, a signature that mentions a tuple would miss
  the type variables inside it during reference validation.
- `comparable_element` (the per-element check §3 folds into `constraint_admits`)
  admits a var element bound `Number` as well as one bound `Comparable`, since
  `number ⊂ comparable`. This is what lets an integer-literal tuple like
  `(1, 2)` compare — its elements are `number` vars, not yet `Int`.
- The paren parsers are spelled `just(LParen).ignore_then(… then_ignore(RParen))`
  rather than `delimited_by(LParen, RParen)`. A naive `delimited_by` disturbed
  chumsky's furthest-error merge and regressed the reserved-constructor field
  diagnostics; the explicit form preserves them.

## Context

Emet has no product type. `Type` is `Var | Rigid | Con | Fun | Record`
(`ast.rs`) — records are the only product. That gap is why the tuple-returning
`elm/core` functions were deferred: ADR 0025 §4 shipped the `Char` primitive and
the Elm-faithful `String`/`Char` surface but held back `String.uncons : String
-> Maybe (Char, String)` and any sibling that returns a pair, because there was
no `(a, b)` to return.

Dr. Dub's decision: add a tuple type **faithful to `elm/core`**, then land the
functions it unblocks. Elm's rules, to replicate:

- Tuples of **exactly 2 or 3 elements**. Elm rejects 4+ at parse time ("cannot
  have more than 3 elements; use a record") and steers the author to a record.
- **Unit `()`** is Elm's zero-tuple — its own type and value.
- `(e)` is grouping, `(e, …)` is a tuple, `()` is unit — one syntactic form,
  three readings by element count.
- Tuples are **comparable** when their elements are; Elm compares them
  lexicographically.
- Elm's **`Tuple` module**: `pair`, `first`, `second`, `mapFirst`, `mapSecond`,
  `mapBoth`.

Ground truth confirmed against the current tree:

- **AST (`ast.rs`).** `Type`, `Expr`, and `Pattern` each have a `Record` arm
  built from a `BTreeMap` — the single-shape product to mirror. A tuple is
  simpler than a record: positional, so a `Vec` not a map, and no `Row` tail (a
  tuple's arity is fixed, never open).
- **Parser (`parser.rs`).** In all three positions the parenthesized form is
  today **pure grouping**: `expr_parser`'s `paren = expr.delimited_by(LParen,
  RParen)` (l. 390), `pattern_parser`'s `paren` (l. 729), and both type parsers'
  `paren` (ll. 156, 224). None admits a comma or an empty `()`. `build_constructor`
  (l. 957) is the established pattern for a semantic parse error via
  `Rich::custom` — the model for the 4-tuple rejection. Records are **not**
  patterns today (`pattern_parser` has no record arm), but that does not block
  tuple patterns: a tuple pattern is its own positional form.
- **Inference (`infer.rs`).** `Type::Record` unifies element-wise
  (`unify_records`, l. 436) — a tuple unifies pointwise by position, strictly
  simpler (equal arity or mismatch; no rows). `apply`/`prune`/`occurs`/`ftv`/`frv`/
  `instantiate_rigids`/`skolemize`/`type_with_param_vars`/`mentions_skolem`/
  `validate_type_refs` each have a `Record` arm that recurses into field types;
  every one needs a parallel `Tuple` arm recursing into element types. The
  `comparable` machinery is **flat**: `constraint_admits` (l. 824) matches only a
  `Type::Con` head and returns `c == Constraint::None` for every non-`Con` type —
  so a tuple is *rejected* by `comparable` as the code stands. Elm-faithful
  structural comparability needs `constraint_admits` to recurse. The
  exhaustiveness checker (`UPat`/`Head`/`lower_pattern`/`specialize`/`useful`/
  `complete_signature`) treats every finite-constructor product as a
  single-constructor sum: `List` rides in as a synthetic 2-constructor sum, a
  user single-variant type as a 1-constructor sum. A tuple is exactly a
  **single-shape product** — one constructor, `arity` sub-patterns.
- **Eval (`eval.rs`).** `Value` (l. 34) has `Record(BTreeMap)` — add
  `Tuple(Vec)`. `match_pattern` (l. 226) destructures records/ctors/lists
  positionally; a tuple arm zips element patterns against element values.
  `compare_values` (`prelude.rs`, l. 375) is closed over `Int/Float/Str/Char` and
  `unreachable!`s otherwise — it needs a lexicographic tuple arm.
- **Depgraph (`depgraph.rs`).** `free_vars_expr` and `collect_pattern_binders`
  must recurse into tuple element expressions/patterns, or a binder inside a
  tuple pattern (`(x, y) -> …`) would be miscounted as a free variable and a
  reference inside a tuple expression missed — a wrong dependency graph.
- **Prelude (`prelude.rs`).** `builtins()`/`ctors()` are the registration
  tables; `A`/`B`/`C` sentinel vars and `fun`/`maybe`/`list` builders are the kit
  for the new `Tuple` module and `uncons`. There is no product builder yet.
- **The wire (ADR 0012/0013, ADR 0025).** `Char` is authoring-time only — it
  never reaches `scroll-format`, because every glyph field lowers to a concrete
  `String`. Tuples are the same: a program still evaluates to `List Scroll`, and
  no glyph field is a tuple. A tuple is a value the language computes *with*, not
  a value any glyph *holds*.

## Decision

### 1. AST: a tuple arm in each of `Type`, `Expr`, `Pattern`; unit is the empty tuple

Add, mirroring each existing `Record` arm but positional (a `Vec`, no `Row`):

```rust
// ast::Type
/// A tuple type `(A, B)` / `(A, B, C)`, or unit `()` when empty. Positional and
/// fixed-arity, so a `Vec` with no `Row` tail — a tuple is never open.
Tuple(Vec<Type>),

// ast::Expr
/// A tuple expression `(a, b)` / `(a, b, c)`, or unit `()`.
Tuple(Vec<Spanned<Expr>>),

// ast::Pattern
/// A tuple pattern `(a, b)` / `(a, b, c)`, or unit `()`. A single-shape
/// product: exhaustive iff its element patterns are.
Tuple(Vec<Spanned<Pattern>>),
```

**Unit is included** (recommended, and confirmed): it is `Tuple(vec![])` in all
three positions — no separate variant. It is Elm-faithful, and it is *free* once
the N-element arms exist: the empty vector falls out of the same construction,
unification, matching, and comparison code with no special case. Including it now
avoids a second ADR later and a jarring "we have tuples but not `()`" gap. The
2-or-3 limit (§2) is a **parser** constraint; the AST vector itself carries any
length, and unit (length 0) is admitted deliberately alongside 2 and 3.

`Display for Type` gains a `Tuple` arm rendering `(A, B)` / `()`.

### 2. Syntax & parsing: the paren form reads by element count; 2-or-3 enforced with a redirect

In each of the three parenthesized parsers (expr, pattern, and both type
parsers), replace the pure-grouping `paren` with a comma-aware form:
`(` then zero-or-more comma-separated inner productions (trailing comma
disallowed, as Elm) then `)`. Dispatch on the count:

- **0 elements** `()` → the unit `Tuple(vec![])`.
- **1 element** `(e)` → **grouping**, unchanged: yield the inner node itself, not
  a 1-tuple. (Elm has no 1-tuple; `(e)` is precedence grouping.)
- **2 or 3 elements** → `Tuple(vec![...])`.
- **4+ elements** → a dedicated semantic parse error via `Rich::custom` at the
  whole-form span (the `build_constructor` mechanism, ADR 0022 — one report per
  error), with the Elm-style redirect:

  > **A tuple can have at most 3 elements.** For a larger grouping, use a record
  > with named fields instead.

Catching 4+ explicitly (rather than letting the extra commas fall through to a
bare "unexpected token") is what guarantees the author sees this message with the
whole tuple underlined — the same rationale as ADR 0026 §3's Float redirect.

The pattern-position form must live at the pattern-`atom` layer where the old
`paren` sat, so `(a, b)` composes with the `::` cons tail and applied
constructors exactly as a parenthesized pattern does today.

The 1-vs-2-vs-many disambiguation is purely by counting the comma-separated
inner list — there is no grammar ambiguity, because `(` unconditionally opens the
form and the element count is known before the node is built.

### 3. Inference: element-wise unification; tuples structurally comparable; authoring-time only

**Unification.** Add a `unify` arm:

```text
(Type::Tuple(as), Type::Tuple(bs)) if as.len() == bs.len()
    => unify each as[i] with bs[i]
```

Unequal arity is a type mismatch (`(a, b)` never unifies with `(a, b, c)` or with
unit) with the standard "expected `{}`, found `{}`" message. This is
`unify_records` with the row cases deleted — position replaces field name, arity
must match exactly, no tail.

**The recursion arms.** Give every `Type`-walking function a `Tuple` arm parallel
to its `Record` arm, recursing into the element vector: `apply`, `prune` (a tuple
needs no row-splicing, so `prune` can treat it like `Con` — deep work is
`apply`'s), `occurs`, `row_occurs`, `ftv`, `frv`, `instantiate` (the inner `go`),
`instantiate_rigids`, `skolemize`, `mentions_skolem`, `type_with_param_vars`, and
`validate_type_refs_inner`. Omitting any one silently drops tuples from that
analysis (e.g. a skipped `ftv` arm would fail to generalize a tuple-typed decl).

**Comparability — the one non-mechanical piece.** Elm makes a tuple `comparable`
iff every element is, and compares lexicographically. The current `Constraint` is
flat — a var is `Comparable` or not — and `constraint_admits` only inspects a
`Con` head, so a `Type::Tuple` is currently inadmissible under `comparable`. Make
comparability **structural** by extending `constraint_admits`:

```text
Constraint::Comparable, Type::Tuple(elems)
    => elems.iter().all(|e| constraint_admits(Comparable, e))
```

Discharge, precisely: when a comparison/equality builtin (`lt`/`eq`/`compare`/…,
all `∀c:comparable. c -> c -> …`) is applied to a tuple, its `comparable` var
unifies with the tuple type; `bind` calls `constraint_admits(Comparable,
tuple)`, which now recurses and admits the tuple exactly when each element type
admits `comparable`. An element that is itself an unresolved `comparable` var is
admitted (it carries the bound forward); a `Con` element defers to the existing
`Int|Float|String|Char` check; a nested tuple recurses. A tuple containing a
function or record element is rejected at that element — the same "does not
satisfy `comparable`" error, now pointing through the tuple. **Unit `()` is
vacuously comparable** (the empty `all` is `true`) and compares `EQ`, matching
Elm. This keeps comparability a *predicate on the resolved type* rather than a
new constraint kind, so no `Constraint` variant is added and `merge_constraints`
is untouched.

**No new type-constructor arity.** Tuples are not a named `Con`, so
`builtin_type_arity` and the `type_arities` set are unchanged; a tuple type is
structural, validated by `validate_type_refs_inner`'s new `Tuple` arm recursing
into elements (each element checked, no head to look up).

**Authoring-time only — never the wire.** Like `Char` (ADR 0025), a tuple is a
value the language computes with; it is not a glyph field and does not appear in
`scroll-format`. `main` is still `List Scroll`, every glyph field still lowers to
a concrete `String`. Therefore: **no `scroll-format`/`golemd` change, and no
`format_version` bump.** This is stated explicitly because the format is
non-self-describing (a field/variant order *is* the encoding) — but tuples add
nothing to that encoding, so the guard does not move.

### 4. Eval: `Value::Tuple`; a `match_pattern` tuple arm; lexicographic `compare_values`

- Add `Value::Tuple(Vec<Value>)` to the `Value` enum, with a `Debug` arm. Unit is
  `Value::Tuple(vec![])`.
- `Expr::Tuple(es)` evaluates each element left-to-right into a
  `Value::Tuple(vs)`.
- `match_pattern` gains a positional arm — a tuple pattern matches a
  `Value::Tuple` of equal length by matching element patterns pointwise (arity is
  guaranteed equal by inference, so this is a zip like the `Ctor` arm; unit
  matches unit with an empty zip that trivially succeeds):

  ```text
  Pattern::Tuple(subs) => match value {
      Value::Tuple(vs) if vs.len() == subs.len()
          => subs.iter().zip(vs).all(|(p, v)| match_pattern(&p.0, v, bindings)),
      _ => false,
  }
  ```

- `compare_values` gains a lexicographic tuple arm: compare element by element,
  returning at the first non-`Equal`; two equal-length tuples that agree
  everywhere are `Equal`. Inference guarantees equal length and element-type
  agreement, so no cross-type or length-mismatch case arises. Unit compares
  `Equal`. This is what makes `<`/`compare`/`==`/`min`/`max` on tuples work — they
  all route through `compare_values`.

### 5. Exhaustiveness: a tuple is a single-shape product — no new completeness logic

A tuple pattern is **one constructor with `arity` sub-patterns** — the record /
single-variant-sum shape the Maranget checker already handles. Reuse it with a
synthetic tuple head, exactly as `List` rides in as the synthetic `[]`/`::` sum:

- `UPat`: `Pattern::Tuple(subs)` lowers to `UPat::Ctor(TUPLE_n, subs')`, where
  `TUPLE_n` is a synthetic constructor name keyed by arity (so a 2-tuple and a
  3-tuple never collide; unit is `TUPLE_0`).
- `complete_signature` for a `Type::Tuple(elems)` scrutinee returns the
  **single**-element signature `[(TUPLE_n, elems.len())]`. Because it is a
  one-constructor "sum", a wildcard column over a tuple is complete once that one
  constructor appears — so a tuple `case` needs **no** catch-all when its element
  patterns are exhaustive, and *is* non-exhaustive when they are not (the
  checker recurses into the element columns and reports there). This is the
  record/single-variant behavior, verbatim.
- `constructor_arg_types(TUPLE_n, tuple_ty)` returns the tuple's element types
  (from the scrutinee, no scheme instantiation) so `useful` recurses into the
  element columns with correct per-column types.

Confirmed against `infer.rs`: `useful`'s wildcard branch already splits on a
complete signature and recurses column-wise; a single-constructor signature makes
a tuple's coverage depend solely on its elements' coverage. **No new arm in the
usefulness algorithm** beyond teaching `complete_signature`/`constructor_arg_types`/
`lower_pattern` about the synthetic tuple constructor — the same three touch-points
`List` uses. So: a `case (x, y) of (0, 0) -> … ; (_, _) -> …` is exhaustive with
no `_` catch-all, and `case p of (Just a, b) -> … ; (Nothing, b) -> …` is
exhaustive iff the `Maybe` column is covered.

### 6. The `Tuple` module + `String.uncons`

Add to `builtins()` (each an Elm-accurate scheme; the runtime builds/reads
`Value::Tuple`). `pair` is the only constructor-style entry; the rest are
accessors/mappers:

| name | scheme | semantics |
|---|---|---|
| `Tuple.pair` | `∀a b. a -> b -> (a, b)` | `\a b -> (a, b)` |
| `Tuple.first` | `∀a b. (a, b) -> a` | first element |
| `Tuple.second` | `∀a b. (a, b) -> b` | second element |
| `Tuple.mapFirst` | `∀a b x. (a -> x) -> (a, b) -> (x, b)` | apply to first |
| `Tuple.mapSecond` | `∀a b y. (b -> y) -> (a, b) -> (a, y)` | apply to second |
| `Tuple.mapBoth` | `∀a b x y. (a -> x) -> (b -> y) -> (a, b) -> (x, y)` | apply to both |

These are `elm/core`'s `Tuple` module exactly, and all operate on the **pair**
(2-tuple) — Elm's `Tuple` module itself has no 3-tuple accessors, so neither do
we (a 3-tuple is destructured by pattern, as in Elm). `Tuple.pair`/`first`/
`second` are the minimum that makes tuples usable without a `case`; the three
`map*` are Elm's and cheap.

Then the original driver, added to the prelude:

- **`String.uncons : String -> Maybe (Char, String)`** — `Nothing` on the empty
  string; on a non-empty string, `Just (firstScalar, rest)` where `rest` is the
  string minus its first Unicode scalar. Scalar-indexed (`chars()`), consistent
  with the rest of the `String` surface (ADR 0025 §5). Its return type is the
  concrete motivation for this whole ADR.

**Scope of the same pass.** `elm/core` has no other tuple-returning `String`
function worth pulling in — `uncons` is the sole one deferred by ADR 0025.
`List.partition : (a -> Bool) -> List a -> (List a, List a)` and `List.unzip :
List (a, b) -> (List a, List b)` are the genuinely-adjacent tuple-returning
Elm functions, but they are **not** currently deferred anywhere and are out of
this ADR's stated scope; adding them is a clean follow-up once tuples land, not a
requirement here. Scope stays tight: the `Tuple` module + `uncons`.

## Alternatives considered

- **Defer unit.** Rejected: unit is `Tuple(vec![])` and falls out of the N-element
  arms for free; deferring it buys nothing and leaves an un-Elm-like gap that
  would need its own later ADR.
- **Tuples not comparable (leave `constraint_admits` flat).** Rejected: Elm
  tuples *are* comparable and authors rely on it (sorting/keying by a pair). The
  structural extension is one recursive arm and adds no `Constraint` variant.
- **Model a tuple as a record with fields `first`/`second` (or `_1`/`_2`).**
  Rejected: it is not Elm's surface (`(a, b)` is its own syntax and type),
  `Tuple.first` would collide conceptually with field access, and positional
  tuples are simpler than rows (no tail, arity-fixed).
- **Allow 4+ tuples.** Rejected: Elm caps at 3 and steers to records for a
  reason — positional readability collapses past three — and the cap is a
  one-line parser check with a helpful redirect (§2).
- **A dedicated `Constraint::ComparableTuple` or a structural constraint kind.**
  Rejected: comparability of a tuple is a *predicate on its resolved element
  types*, not a new bound to thread through `merge_constraints`. Recursing in
  `constraint_admits` keeps the constraint set the same closed four.

## Consequences

- Emet gains a product type in expression, pattern, and type position, plus unit
  — the `(a, b)`/`(a, b, c)`/`()` Elm surface — with the 2-or-3 cap enforced at
  parse time and a record redirect for 4+.
- Tuples are structurally comparable and compare lexicographically, so `<`,
  `compare`, `==`, `min`, `max` all work on them; unit is vacuously comparable
  and `EQ`. The `comparable` admissibility check becomes recursive but the
  `Constraint` set is unchanged.
- A tuple `case` needs no catch-all when its element patterns are exhaustive,
  because a tuple is a single-shape product — the same rule as records /
  single-variant sums, with no new usefulness-algorithm logic (only the three
  `List`-style synthetic-constructor touch-points).
- The `Tuple` module (`pair`/`first`/`second`/`mapFirst`/`mapSecond`/`mapBoth`)
  and `String.uncons` ship, closing the ADR 0025 §4 deferral.
- **Forecloses / holds the line:** tuples are authoring-time only — no glyph
  field is a tuple, `main` stays `List Scroll`, `scroll-format`/`golemd` are
  untouched, and there is **no `format_version` bump**. The four
  reconciler-owned glyph kinds are unchanged; a tuple is not a fifth resource
  kind, it is a language value. No 1-tuple (that is grouping) and no 4+-tuple
  (that is a record) ever enter the AST.
- Implementation surface (plan, not code):
  - `ast.rs` — three `Tuple` variants + `Display` arm.
  - `parser.rs` — the comma-aware paren form in `expr_parser`, `pattern_parser`,
    `type_parser`, and `type_atom_parser` (count-based dispatch: unit / grouping
    / tuple / 4+-error); the redirect message constant.
  - `infer.rs` — the `unify` tuple arm; `Tuple` arms in `apply`/`prune`/`occurs`/
    `row_occurs`/`ftv`/`frv`/`instantiate`/`instantiate_rigids`/`skolemize`/
    `mentions_skolem`/`type_with_param_vars`/`validate_type_refs_inner`; the
    recursive `constraint_admits` comparable case; the exhaustiveness
    touch-points (`lower_pattern`, `complete_signature`, `constructor_arg_types`)
    for the synthetic `TUPLE_n` constructor.
  - `eval.rs` — `Value::Tuple` + `Debug`; `Expr::Tuple` eval; `match_pattern`
    tuple arm.
  - `prelude.rs` — the lexicographic `compare_values` tuple arm; the `Tuple.*`
    builtins and `String.uncons`.
  - `depgraph.rs` — `free_vars_expr` `Expr::Tuple` arm and
    `collect_pattern_binders` `Pattern::Tuple` arm.
  - Tests (Elm-parity): build/destructure 2- and 3-tuples; unit build/match;
    nested tuple patterns (`(Just a, (b, c))`); a tuple `case` exhaustive with no
    catch-all, and the non-exhaustive counterpart flagged; tuple comparison and
    ordering; the 4-tuple rejection message + span; `String.uncons` on empty and
    non-empty; each `Tuple` module function.
- This will be **implemented next**.

## Cross-references

- ADR 0025 — the `Char` primitive and the `String`/`Char` surface; §4 deferred
  `String.uncons` for want of a tuple. `Char` is the precedent for an
  authoring-time-only type that never reaches the wire.
- ADR 0026 — literal patterns; the `Rich::custom` semantic-parse-error mechanism
  and the "helpful redirect" pattern reused for the 4-tuple message.
- ADR 0010 — row-polymorphic records; the product `Type::Record` this mirrors
  (tuples are the positional, tail-free, fixed-arity product).
- ADR 0005 — `case`, exhaustiveness/redundancy (the Maranget checker); tuples
  reuse its single-shape-product path, as `List` reuses its synthetic-sum path.
- ADR 0007 — `number`/`comparable`/`appendable`; the `comparable` constraint
  tuples now discharge structurally.
- ADR 0012/0013 — the binary content-addressed manifest and `scroll-format`; the
  wire the tuple type deliberately does not touch (no `format_version` bump).
