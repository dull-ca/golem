# 0003-generics-type-variables-and-type-application

## Status

Proposed

## Context

Emet's core inference (`src/infer.rs`, Algorithm W) is **already parametrically
polymorphic**: `generalize` / `instantiate` / `unify` / `ftv` / `occurs` all
operate over `Type::Var`, and `id x = x` already generalizes to `∀t. t -> t` and
is used at two types (`tests/pipeline.rs::hm_infers_polymorphic_identity`). The
gap is **surface syntax and type representation**, not the inference math.

Two concrete limits block modeling Elm's type language:

1. **No surface type variables.** `Type::Var(u32)` exists *only* as a
   unification variable; there is no way to *write* `a` in a signature. The
   signature-check path in `infer_decls` even documents the assumption —
   "The signature uses concrete types only (no variables), so unify" — and
   unifies the annotation directly against the inferred body. This is correct
   only for ground signatures and is wrong the moment a signature says `a`.

2. **No type application.** `Type` enumerates fixed heads (`Str`, `AptPackage`,
   `SystemdService`, `Glyph`, `Glyphs`); `type_parser` hard-codes those names
   and only `[Glyph]`. There is no `List a`, `Maybe a`, or general
   `Constructor arg…`.

We want Elm-accurate signatures: `map : (a -> b) -> List a -> List b`,
`Maybe.withDefault : a -> Maybe a -> a`, etc.

## Decision

**Generalize the type representation and let signatures mention type variables
and applied type constructors, reusing the existing HM core unchanged.**

### Type representation

Replace the fixed-head `Type` with a uniform applied-constructor node, a rigid
signature-variable node, and a **bound-carrying** unification variable:

```rust
pub enum Type {
    Var(u32, Constraint),           // unification variable + optional bound
    Rigid(String),                  // a signature's `a`, `b`, … (transient; see below)
    Con(String, Vec<Type>),         // applied type constructor, arity = args.len()
    Fun(Box<Type>, Box<Type>),
    Record(BTreeMap<String, Type>),
}

pub enum Constraint { None, Number, Comparable }
```

- `Con(String, Vec<Type>)` subsumes every current concrete type as a nullary
  constructor (`Con("String", [])`, `Con("Glyph", [])`, …) and expresses
  application (`Con("List", [t])`, `Con("Maybe", [t])`). The bespoke `Glyphs`
  becomes `Con("List", [Con("Glyph", [])])`.
- `Rigid(String)` represents a signature type variable. It appears only while
  checking one declaration's signature and never leaks into general inference.
- **`Var(u32, Constraint)` carries an optional bound**, decided **now** even
  though only `Constraint::None` is minted until numbers land (ADR 0007). This
  is a deliberate "pick the representation once, never repaint it" call: numbers
  are in scope, and their bounded `number`/`comparable` vars must not force a
  later `Type`/`Var` rewrite. `bind` threads the bound; `Scheme` stores a bound
  per quantified var (`Vec<(u32, Constraint)>` or a parallel map) so a
  generalized `number` re-instantiates as `number`. Enforcement (the
  admissibility table, literal typing, defaulting) is ADR 0007's; the
  representation and plumbing are this ADR's.

`unify` / `apply` / `ftv` / `occurs` / `Display` fold their concrete arms into
`Con` (recurse over the arg vector exactly as they already recurse into `Fun`
and `Record`). The **glyph-subsumption arms from ADR 0002 are preserved
verbatim** as explicit `Con`-head special cases: `AptPackage`/`SystemdService`
each unify with `Glyph`, and *not* with each other. This is the **interim
symmetric injection** — sound only while nothing eliminates glyphs; ADR 0008
records the deferred principled replacement (see also the design doc §5).

### Signatures with type variables

- `type_parser` accepts a lowercase ident as a type variable → `Rigid(name)`
  (all `a`s in one signature share `Rigid("a")`), and an applied head
  `Name t1 t2` → `Con("Name", [t1, t2])`, with arity checked against known type
  constructors.
- `infer_decls` checks a signature by **instantiating its `Rigid`s to fresh
  unification `Var`s** and unifying that against the inferred body. Because the
  body's own free vars then unify with those, `generalize` quantifies them and
  the decl becomes polymorphic — the same machinery that already makes `id`
  polymorphic.

### Naming (decided)

Adopt Elm's `String` as the type spelling (`Con("String", [])`), with `Str`
retained as a transitional surface alias for **one migration step** so existing
`.emet` files and tests do not all churn at once, then remove the alias. This
is settled, not an open question.

**As implemented:** that migration step is complete — the `Str` and `Glyphs`
aliases are **removed**. `String` and `List Glyph` are the sole spellings;
`Str`/`Glyphs` now parse as ordinary `Con` heads and fail as unknown type
constructors (`parser::type_con` special-cases nothing).

## Alternatives considered

1. **Separate `App(Box<Type>, Box<Type>)` + `Con(String)` nodes** (curried type
   application, à la GHC core). Rejected: Emet's type constructors are always
   fully applied in surface syntax; a flat `Con(name, args)` is simpler to
   unify (compare head + arity, recurse) and matches how the checker reasons.
2. **Keep fixed heads; add only `Var`-from-signature.** Rejected: cannot express
   `List a` / `Maybe a`; does not model Elm.
3. **Full skolem-escape checking in the first increment** (reject
   *over-general* signatures like `f : a -> a` on a monomorphic body).
   Deferred: the instantiate-and-unify check accepts every *correct* program and
   only fails to *reject* an over-general annotation — a soundness-of-rejection
   gap, not an evaluation bug. Skolem-escape checking is a later hardening step.
   **Since implemented — ADR 0021** adds it atop this instantiate-and-unify
   check, closing that gap.

## Consequences

- One `Type::Con` + `Var(u32, Constraint)` refactor is the keystone for `List`,
  `Maybe`, `Bool`, **numbers** (ADR 0007), and future primitives; adding a type
  constructor is now data, not a new enum arm, and the bounded-var machinery is
  in place before numbers need it (no repaint).
- The HM core (`generalize`/`instantiate`/`unify`/`ftv`/`occurs`) is unchanged in
  algorithm; only its pattern-matching is refactored onto `Con`, the bound is
  threaded through `bind`, and a `Rigid` case is added at the signature boundary.
- `Str → String` is **decided** and the migration is **done** — the transitional
  `Str`/`Glyphs` aliases are removed; `Str`/`Glyphs` are now unknown-type errors.
- Over-general signatures were accepted until skolem-escape checking landed
  (documented, bounded, non-evaluation-affecting); **ADR 0021** now rejects them.
- Preserves ADR 0002: glyph subsumption becomes explicit `Con`-head arms; the
  symmetric injection is **interim** and superseded by ADR 0008 before any glyph
  elimination.
- Cross-references ADR 0007 (numbers / constrained vars), ADR 0008 (glyph
  pattern-matching), ADR 0021 (skolem-escape check for over-general signatures),
  and the design doc
  `docs/design/0001-elm-lite-type-system-and-value-language.md` §4.
