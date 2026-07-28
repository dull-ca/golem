# 0007-numbers-constrained-type-variables-and-infix-operators

## Status

Proposed. **Amended:** `++`/`appendable` is no longer special-cased or deferred
— `Appendable` is now a real third `Constraint` alongside `number`/`comparable`
(see the Decision's `(++)` bullet and Alternative 4).

## Context

Emet is modeling Elm as closely as practical. Numbers are now **in scope and
modeled after Elm faithfully** — `Int`, `Float`, Elm's `number`/`comparable`
bounded type variables, and infix operators with Elm precedence. This reverses an
earlier framing that treated numbers and infix operators as non-goals.

Three facts shape the decision:

1. The lexer has **no numeric literals** (no integer/float tokens) and **no
   operator symbols** beyond `-> = \ : , . ( ) [ ] { }`. Both are green-field.
2. `number`/`comparable` are the **one place Elm leaves pure Hindley-Milner**: a
   unification variable may carry a *bound* (typeclass-*like*, but a closed,
   built-in set), so `3 : number` is usable at `Int` or `Float`, and `(<)`
   works for any `comparable`.
3. There is **no user recursion** (totality), so the numeric/`comparable`
   functions a user would write recursively in Elm cannot be written in-language;
   they ship as total built-in primitives, exactly like `List.map` (ADR 0006).

The type-representation cost is paid in ADR 0003's Wave-0 refactor, which already
introduces `Var(u32, Constraint)` so this ADR adds no `Type` repaint — only
enforcement, literal typing, operators, and builtins.

## Decision

**Add `Int`/`Float`, Elm's `number`/`comparable` bounded type variables, and
Elm-precedence infix operators — following Elm exactly this far and no further
(no user-defined typeclasses, no user-defined operators).**

### Types and constraints

- `Con("Int", [])`, `Con("Float", [])`.
- `Constraint::{None, Number, Comparable}` on `Var` (representation from ADR
  0003). Admissibility:
  - `Number` admits `Int`, `Float`.
  - `Comparable` admits `Int`, `Float`, `String` (and `Char`/`List
    comparable`/tuples if those types are ever added — not required now).
  - `Number ⊂ Comparable`.
- `bind` enforces the bound: binding a bounded var to an inadmissible concrete
  (`Number` ← `String`) is a type error; binding two bounded vars merges to the
  stronger bound (`Number ∧ Comparable = Number`; `None ∧ c = c`).
- `generalize`/`instantiate` carry the bound; `Scheme` stores per-var bounds so a
  generalized `number` re-instantiates as `number`.

### Literals and defaulting

- Integer literal `3` → fresh `Var(_, Number)` (`3 : number`).
- Float literal `3.0` → `Con("Float", [])` (Elm: float literals are `Float`).
- **Defaulting:** an unresolved `number` after inference **defaults to `Int`**
  (Elm's behaviour), applied at top-level-decl generalization and at `main`.

### Infix operators (Elm precedence)

- Lexer gains maximal-munch operator symbols
  `+ - * / // ^ < > <= >= == /= && || ++`, emitted as `Tok::Op`. `->` and `--`
  keep priority (arrow, comment).
- Parser gains one precedence-climbing (Pratt) layer between application and the
  low-precedence forms, with Elm-accurate precedence/associativity:

  | Prec | Ops | Assoc |
  |---|---|---|
  | 7 | `^` | right |
  | 7 | `*` `/` `//` | left |
  | 6 | `+` `-` | left |
  | 5 | `++` | right |
  | 4 | `==` `/=` `<` `>` `<=` `>=` | non-assoc |
  | 3 | `&&` | right |
  | 2 | `||` | right |

  (`not` is a prefix *function*, not an operator.) Non-associative level 4 makes
  `a < b < c` a parse error, as in Elm.
- **Operators desugar to prelude builtin applications** (`a + b` → `add a b`,
  `a ++ b` → append, `a == b` → `eq a b`), so `infer.rs`/`eval.rs` see only
  ordinary `App`. Types come from the builtins:
  `(+) : number -> number -> number`, `(<) : comparable -> comparable -> Bool`,
  `(==) : comparable -> comparable -> Bool`.
- `(++)` is Elm's `appendable` (`String`/`List`). **As implemented, this is a
  real third `Constraint::Appendable`** (not the special-case originally
  proposed): `++` desugars to an `append` builtin typed
  `∀p:appendable. p -> p -> p`, `constraint_admits` accepts `String`/`List`, and
  the builtin dispatches String vs. List on the runtime value at eval time.
  `merge_constraints` treats `appendable` as disjoint from `number`/`comparable`
  (they share no admissible type), so `appendable ∧ number` and
  `appendable ∧ comparable` are unsatisfiable and rejected.

### Builtins (total; ADR 0006 prelude)

Elm exposes these **unqualified** (`round`, `abs`, `min`, …); Emet binds them as
bare prelude names — the one exception to "qualification is mandatory" (ADR
0006), because Elm itself exposes them bare:
`toFloat, round, floor, ceiling, truncate, negate, abs, modBy, remainderBy,
clamp, min, max, compare` (with `compare : comparable -> comparable -> Order`,
`Order = LT | EQ | GT` an ordinary sum type). Un-defers the numeric/equality
`List.length/range/sum/maximum/minimum`, `List.member`,
`String.length/fromInt/fromFloat/toInt/toFloat`.

### Totality of division

`//`, `modBy`, `remainderBy` are **total**: division/modulo by `0` is *defined*
(Elm returns `0`), never a trap. Emet adopts Elm's exact behaviour so evaluation
cannot crash.

## Alternatives considered

1. **No numbers / minimal `Int` subset.** Rejected by the decision to model Elm
   fully; a partial numeric story is neither Elm-faithful nor "nice".
2. **Full user-definable typeclasses / `where`-constraints / dictionaries.**
   Rejected: far beyond Elm-lite; `number`/`comparable` are a closed, built-in
   set exactly as in Elm — no user extension.
3. **User-defined operators (custom fixity).** Rejected: a non-goal; only Elm's
   built-in operator set with fixed precedence.
4. **A third `Appendable` constraint for `++`.** Originally deferred in favor of
   special-casing `++`. **Superseded:** `Appendable` is now the implemented
   design — a real third constraint (`String`/`List`) carried on `Var` and
   threaded through `bind`/`merge_constraints`/`constraint_admits` exactly like
   `number`/`comparable`, with runtime String/List dispatch in the `append`
   builtin.
5. **Trap on division by zero.** Rejected: breaks totality; Elm's defined-at-zero
   behaviour is adopted.
6. **Deciding the constrained-`Var` representation in this ADR.** Rejected — it
   is decided in ADR 0003's Wave-0 refactor precisely so numbers never force a
   `Type`/`Var` repaint.

## Consequences

- Emet gains Elm-faithful numbers, bounded polymorphism, and operators while
  staying total (operators are strict total builtins; division defined at zero).
- The only departure from pure HM is two closed bounded constraints, threaded
  through `bind`/`generalize`/`instantiate` via ADR 0003's representation.
- Operators are pure surface sugar over prelude builtins — no new inference or
  eval nodes.
- New open questions (all low-risk, in the design doc §14): where `number`
  defaulting runs; `++` special-case vs. `Appendable`; confirming Elm's
  division-at-zero semantics.
- Sequenced as design-doc Wave 5 (after `case`/`Bool`/`Order`, before
  interpolation), with type-representation cost pre-paid in Wave 0.
- Cross-references ADR 0003 (constrained-`Var` representation), ADR 0006
  (builtins/prelude), and the design doc `docs/design/0001-…` §4.4, §9.5, §10.2.
