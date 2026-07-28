# 0021-skolem-escape-check

## Status

Accepted 2026-07-21; implementation landed.

## Context

ADR 0003 shipped signatures with type variables on the *instantiate-and-unify*
plan: a signature's `Rigid` vars become fresh unification `Var`s, that
instantiated signature unifies against the inferred body, and generalization
quantifies the survivors. Design §4.3 named the half it left out. Instantiation
accepts every *correct* program, but it cannot reject a signature *more general*
than the body — `f : a -> a` over `\x -> x + 1`, or a signature that keeps two
variables distinct where the body forces them equal. Instantiating `a` to a
fresh `Var` lets that `Var` unify with whatever the body demands, so the
annotation's over-claim never surfaces.

Both ADR 0003 and design §4.3 recorded this as a bounded, known gap and deferred
the fix. It is a soundness-of-*rejection* gap, not an evaluation bug: Emet
generalizes the *inferred* type, so a wrongly-general signature still evaluates
correctly — the compiler simply fails to tell the author their annotation
overpromises. Everything else in the type system now stands (numbers, glyph
matching, modules, the LSP), and this is the last piece of the ADR 0003 story
still open.

## Decision

**Add skolem-escape checking on top of the instantiate-and-unify check, and
define an over-general signature as one the instantiate pass accepts but the
skolemize pass rejects** (`infer::check_signature_generality`).

### Keep both passes

Instantiate and skolemize answer different questions, and both are needed:

- **Instantiate** turns each signature var into a fresh `Var` that unification
  may bind. It accepts every correct program — the "does the body fit the
  signature at all?" question — and its unify failures carry the original,
  specific "type mismatch" messages.
- **Skolemize** turns each signature var into a fresh, globally unique *rigid
  constant* (`skolem$n`). A rigid cannot be bound away, so forcing a skolem to a
  concrete type, unifying two distinct skolems, or letting a skolem escape into
  the surrounding environment all fail. That is precisely the "is the signature
  more general than the body?" question.

A signature is over-general **iff the instantiate pass accepts it but the
skolemize pass rejects it**. Defining it as the *disagreement* between the two,
rather than replacing instantiate with skolemize outright, is what preserves the
original error messages: an ordinary shape mismatch fails *both* passes and
keeps its "type mismatch" wording; only the strictly over-general case — accepted
by instantiate, rejected by skolemize — is relabelled "signature is too general".

### What "rejected by skolemize" means

The skolemize pass fails if either the unify itself fails (a skolem was forced to
a concrete type or to a different skolem) **or** a skolem escaped: it surfaced in
a type variable that was free in the surrounding environment *before* the check
ran (`skolems_escape_into` / `mentions_skolem`). The escape scope is deliberately
those *pre-existing* outer free vars only — not the inference group's own
recursion vars, which are bound monomorphically in the body env. A self- or
mutually-recursive decl calling itself at its own type is therefore not mistaken
for an escape, so polymorphic recursion still passes.

### The bounded-name exemption

`number`, `comparable`, and `appendable` are Elm's bounded type variables (ADR
0007), not universally quantifiable ones. They skolemize to a fresh
*constrained* `Var`, not a rigid (`bounded_variable_constraint`). Treating them
as rigid would reject correct programs — `double : number -> number` over
`\x -> x + x` legitimately forces the bound to a concrete numeric type — so they
ride their existing bound through `bind` instead.

### Snapshot isolation is load-bearing

Both trial passes run on a throwaway copy of the state: `subst`, `row_subst`, and
`next` are snapshotted before the passes and restored before returning. Neither
the instantiate binding nor the skolemize binding survives. The real signature
unification and generalization in `infer_group` run afterward against untouched
state, so this check only decides *whether* to raise the error — never what type
the declaration finally gets.

## Alternatives considered

1. **Replace instantiate-and-unify with skolemize-and-unify.** Rejected: the
   skolemize pass's unify failures carry generic messages, so an ordinary shape
   mismatch would lose its specific "type mismatch: expected … found …" wording.
   Running instantiate first and reserving "signature is too general" for the
   two passes' disagreement keeps every existing error message intact.
2. **Leave the gap open (status quo).** Rejected: it was always meant as a
   staged deferral (ADR 0003, design §4.3), and it is the last open piece of the
   generics story. The gap never risked evaluation — Emet generalizes the
   inferred type — but it silently accepted annotations that overpromise, which
   is exactly the kind of mistake a signature exists to catch.
3. **Skolemize the bounded names to rigids too.** Rejected: `number` and friends
   carry a bound the body may legitimately satisfy at a concrete type, so a rigid
   skolem would reject correct numeric code. They map to a constrained `Var`.

## Consequences

- Over-general signatures are now compile errors: `f : a -> a` over `x + 1`, a
  signature keeping two vars distinct that the body forces equal, and a var the
  body forces to a concrete type are all rejected as "signature is too general",
  while `id` / `const` / `map` and polymorphic recursion still pass.
- Ordinary shape mismatches are unchanged — they fail both passes and keep their
  original "type mismatch" message; only the strictly over-general case is
  relabelled.
- Never an evaluation-soundness change. Emet generalizes the inferred type, so
  the check only converts a previously-accepted-but-wrong annotation into an
  error; no program that used to evaluate now evaluates differently.
- The check is purely additive and side-effect-free thanks to snapshot
  isolation: it runs before the real signature unification in `infer_group` and
  restores all inference state, so real generalization is untouched.

## Cross-references

- ADR 0003 (generics / type variables / type application) — introduced the
  instantiate-and-unify signature check and deferred this hardening step; the gap
  it named is closed here.
- ADR 0007 (numbers / constrained type variables) — the bounded-name exemption.
- ADR 0011 (general recursion, SCC-grouped inference) — the group recursion vars
  that the escape scope deliberately excludes.
- Design doc `docs/design/0001-elm-lite-type-system-and-value-language.md` §4.3 —
  the original description of the deferred skolem-escape check.
