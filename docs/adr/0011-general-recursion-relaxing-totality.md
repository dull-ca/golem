# 0011-general-recursion-relaxing-totality

## Status

Accepted.

## Context

Emet was total by construction: a declaration's own name is not in scope
within its own body, because declarations are inferred and evaluated
left-to-right. Users could therefore not write a recursive function — they
were limited to the built-in combinators (`List.map`, `List.foldr`, …). This
was a deliberate totality guarantee: guaranteed termination and a finite
resource DAG (see the "Total language" invariant in `CLAUDE.md` and the
totality discussion in `docs/design/0001-elm-lite-type-system-and-value-
language.md`).

In practice this is the single biggest ergonomic limitation for a
functional-feeling language: users cannot write `factorial`, `fib`, or a
recursive walk over a list or (once user `type` declarations and their
patterns land, ADR 0005) a user ADT, without reaching for a builtin that
happens to already do the traversal they need. The project has decided to
**prioritize usefulness and ergonomics over totality**.

## Decision

**Allow general (self-)recursion.** A top-level declaration's name is now in
scope within its own body, so users can define recursive functions directly.

- **Inference** uses the standard recursive-let rule: bind the declaration's
  name to a fresh monomorphic type variable while checking its body, then
  generalize once the body has been checked.
- **Evaluation** ties the knot per application, not via a cyclic environment
  structure: a closure carries its own name and re-binds itself into its own
  environment when applied.
- **This relaxes totality.** Evaluation is no longer guaranteed to terminate
  — a user can write a non-terminating function (e.g. `loop x = loop x`).
  Totality becomes a *soft preference*, not an invariant. A recursion-depth
  guard MAY be added so the compiler fails cleanly ("evaluation exceeded
  recursion limit") instead of hanging or crashing on accidental infinite
  recursion; this is a responsiveness safety net, not a totality guarantee.
- **Exhaustiveness checking for `case` is retained** (ADR 0005) — it is cheap
  safety and is independent of termination; there is no reason to give it up.
- **Scope: self-recursion only.** A declaration may call itself; a
  declaration referencing a *later* declaration (mutual recursion) remains
  unsupported, since decls are still inferred/evaluated left-to-right. Mutual
  recursion is a follow-up, not part of this decision.

## Alternatives considered

1. **Keep totality (no user recursion).** Rejected: this is the primary
   ergonomic gap in the language today; users cannot write ordinary
   recursive functions, which undercuts the "functional configuration
   language" pitch.
2. **Structural / bounded (well-founded) recursion only** (e.g. recursion
   restricted to a strictly-decreasing structural argument, in the style of
   total functional languages). Rejected: hard to explain to users, overly
   restrictive in practice, and not how Elm or ordinary functional code
   reads — poor ergonomics for the payoff versus just allowing recursion.
3. **Fuel-limited evaluation as a totality substitute** (every recursive
   call consumes fuel from a fixed budget; termination is enforced by
   construction). Not adopted as a guarantee — it would still reject valid
   deep-but-finite recursion or require awkward fuel threading. A depth
   guard may be used later purely as a responsiveness safety net, not as a
   replacement for real totality.

## Consequences

- Users can write recursive functions — a major ergonomic gain, and the
  natural way to consume user-defined sum types (ADR 0005) once user `type`
  declarations exist.
- Evaluation may not terminate: "guaranteed termination" and "finite
  resource DAG" no longer hold as invariants. A depth guard, if added,
  mitigates hangs/crashes but is not a soundness or totality mechanism.
- The glyph no-elimination soundness argument (ADR 0002) is unaffected — it
  concerns pattern-matching glyphs, not termination, and nothing here
  changes glyph elimination.
- Supersedes the "Total language" invariant framing in `CLAUDE.md` and the
  totality claims in `docs/design/0001-elm-lite-type-system-and-value-
  language.md`; those documents are updated to point here rather than
  restate termination as guaranteed.
- Mutual recursion stays out of scope for now; left-to-right decl ordering
  is unchanged outside of a declaration's own body.
