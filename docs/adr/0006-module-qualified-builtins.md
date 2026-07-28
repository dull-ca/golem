# 0006-module-qualified-builtins

## Status

Proposed

## Context

Modeling Elm means offering `List.map`, `List.filter`, `List.foldr`,
`List.concatMap`, `Maybe.map`, `Maybe.withDefault`, `Maybe.andThen`,
`String.concat`, `String.join`, `String.append`, and friends — with
Elm-accurate generic signatures.

Two forces shape how Emet must provide them:

1. **No user recursion (totality).** In Elm these functions are ordinary library
   source a user *could* write recursively. In Emet the user **cannot** write
   them — there is no recursion — so iteration over a `List` is only possible
   through primitives the compiler supplies.
2. **No real module system.** Emet has no modules, imports, or record-valued
   namespaces, and adding one is out of scope for "Elm-lite."

So `List.map` must be a **built-in**, and `List.`/`Maybe.`/`String.` must be a
naming convention rather than real modules.

## Decision

**Implement `List.` / `Maybe.` / `String.` as compile-time-resolved qualified
names bound to total Rust built-ins in a prelude — not real modules, not
records.**

- **Parsing.** `Tok::Upper` immediately followed by `Tok::Dot` and `Tok::Ident`
  parses as a single qualified identifier, e.g. `Expr::Var("List.map")`. Because
  it becomes a plain `Expr::Var` with a dotted name, there is **no new `Expr`
  node and no eval change** — it resolves through ordinary environment lookup.
  Adjacency (via spans) distinguishes `List.map` from field access `.f`.
- **Disambiguation by mandatory qualification.** There is no bare `map` in the
  prelude — only `List.map` and `Maybe.map` — so qualification is required and
  there is no ambiguity. This matches explicit-qualified Elm usage.
- **Prelude.** A `prelude` module returns a seeded `(TyEnv, Env)`:
  the glyph constructors, the sum-type constructors (`Just`/`Nothing`/`True`/
  `False`), and every qualified builtin bound to a total Rust function.
  `check_module` starts from the prelude `TyEnv`; `run_module` from the prelude
  `Env`. Builtins are a `Value::Builtin { name, arity, apply }` (or curried
  closures over Rust fns); the `App` arm collects args until saturated, then
  calls the Rust fn. Higher-order builtins apply user `Value::Closure`s via the
  existing apply path.
- **Signatures are Elm-accurate**, e.g.
  `List.map : (a -> b) -> List a -> List b`,
  `List.foldr : (a -> b -> b) -> b -> List a -> b`,
  `List.concatMap : (a -> List b) -> List a -> List b`,
  `Maybe.withDefault : a -> Maybe a -> a`,
  `Maybe.andThen : (a -> Maybe b) -> Maybe a -> Maybe b`,
  `String.concat : List String -> String`,
  `String.join : String -> List String -> String`.
- **Numeric-dependent functions deferred.** `List.length`, `List.range`,
  `String.length`, and `List.member` (needs equality) are deferred until a
  numeric/equality story exists; they are *not* required for a usable core.

## Alternatives considered

1. **Real modules / imports.** Rejected: large feature, out of scope for
   Elm-lite; qualified-name resolution gives the Elm *surface* with none of the
   module machinery.
2. **Records as namespaces** (`List` is a record of functions;
   `List.map` is field access). Rejected: makes `List` a first-class value,
   complicates typing and eval, and invites partial `List` records; the dotted
   name is simpler and closed.
3. **Bare (unqualified) builtins** (`map`, `filter`, `concat`). Rejected:
   `map`/`concat` collide across `List`/`Maybe`/`String`; mandatory
   qualification is unambiguous and Elm-idiomatic.
4. **Write the combinators in Emet source.** Impossible: no recursion. This is
   precisely why they must be built-ins (see §Context).

## Consequences

- Users write exactly `List.map f xs` (Elm naming/semantics); only the
  *implementation locus* differs — the combinator is a Rust builtin, not library
  source. This is a divergence-in-mechanism, documented as such.
- The builtin set is the language's iteration vocabulary; it grows by adding
  prelude entries (scheme + Rust fn), not by new AST/eval nodes.
- `String.concat` is the desugar target for string interpolation (ADR 0004), so
  the `String` builtins and interpolation are ordered together.
- No new crate; all builtins are Rust over `Value`.
- Cross-references ADR 0004 (interpolation → `String.concat`) and the design doc
  `docs/design/0001-…` §10.
