# 0008-glyph-pattern-matching-deferred

## Status

**Superseded by ADR 0017.** This ADR recorded the deferral and named its
preferred successor — route 2, directed nominal subsumption. ADR 0017 is that
route landed: the symmetric injection arm was replaced by one-way widening
(concrete glyph → `Glyph`), and `case` now matches glyphs and the filesystem
`Entry`. The context and the two routes below remain the record of why.

## Context

ADR 0002 makes `Glyph` a sum of concrete glyph types (`AptPackage`,
`SystemdService`, and soon `File`/`LineInFile`) via a **permissive, symmetric**
unification arm: each concrete glyph unifies with `Glyph`, in both directions,
without tracking which variant a `Glyph`-typed value actually is. ADR 0002 notes
this is sound **only while glyphs have no elimination form**.

As the language grows a real elimination form — `case … of` (ADR 0005) — it is
worth being precise about what is and isn't safe, and to avoid foreclosing a
future where glyphs can be pattern-matched.

**Corrected framing (the important part).** Matching a glyph is *not* inherently
unsafe: pattern-matching a sum value is a trivial `case`, and if `Glyph` were an
ordinary sum it would be perfectly sound. The interim unsoundness is specifically
the **symmetric injection under elimination**: because the ADR-0002 arm lets a
`Glyph`-typed hole be satisfied by *either* concrete glyph without recording
which, an elimination that asks "which glyph is this?" could inspect a value
whose concrete identity was never pinned down. The shortcut — not matching — is
the thing to replace.

## Decision

**Defer glyph pattern-matching. Keep the interim concrete-subtype + injection
model (sound while nothing eliminates glyphs), and keep the Wave-0 foundation
chosen so a principled model is additive, not a repaint.**

- **Near term:** no glyph constructors/patterns exist to write; the ADR-0002
  symmetric injection stays. Constructors keep returning their **precise
  subtype** (`aptPackage … : AptPackage`, `… -> SystemdService`), so subtype
  precision is retained, not erased.
- **Precondition on any future glyph elimination:** the symmetric injection arm
  **must be replaced by a principled model first**. No glyph `case` is added
  while the symmetric arm is live.
- **Foundation kept variant-ready:** the Wave-0 `Con`-head representation (ADR
  0003) already models `Glyph` as a named constructor with concrete constructors
  relating to it — the shape both principled routes below extend. The
  `case`/exhaustiveness machinery (ADR 0005) is the elimination machinery a glyph
  model would reuse.

Two principled routes (sketched, **not designed here**):

1. **Polymorphic / row variants.** Model `Glyph` as an open variant/row;
   constructors inject with precise row types; `case` matches tags soundly with
   row-based exhaustiveness. Most faithful to "typed sum with subtyping"; heavier
   machinery (row unification).
2. **Lightweight nominal subtyping with directed subsumption.** Keep nominal
   `AptPackage <: Glyph`, but replace the *symmetric* unify arm with a
   **directed** subsumption check (concrete → `Glyph` only, never the reverse)
   plus an elimination form that requires the scrutinee be the sum type and
   matches nominal tags.

**Recommendation when the time comes: route 2 (directed nominal subsumption).**
Glyphs are a closed, compiler-owned set (not user-extensible rows), so directed
nominal subsumption is the smaller, more Emet-shaped step; it removes exactly
the one unsound thing (the symmetric arm) while keeping the existing nominal
`Con` heads. Reserve route 1 for a hypothetical future of user-defined open
variants (not in scope).

## Alternatives considered

- **Forbid glyph matching permanently.** Rejected: unnecessary; matching is not
  the hazard, and closing the door loses a plausibly-useful capability.
- **Design the full variant/row system now.** Rejected: premature; no current
  feature needs it, and it is a large piece. This ADR only preserves the option.
- **Make constructors return `Glyph` (erase the subtype).** Rejected: loses
  subtype precision that signatures already rely on (`-> SystemdService`).

## Consequences

- The permissive injection of ADR 0002 is explicitly **interim**, with a named
  successor and a stated precondition (replace before any glyph elimination).
- No implementation cost now; the obligation is a design constraint on the
  Wave-0 foundation (already satisfied) and a gate on any future glyph `case`.
- Cross-references ADR 0002 (interim symmetric injection), ADR 0003 (`Con`
  foundation), ADR 0005 (elimination machinery / exhaustiveness), and the design
  doc `docs/design/0001-…` §5.
