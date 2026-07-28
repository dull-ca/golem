# 0010-row-polymorphic-records

## Status

Accepted.

## Context

Record field access only type-checked when the base was already a concrete
record. A function parameter is an unbound type variable during bottom-up
inference, so `\h -> h.name` and record-parameter helpers — even *with* a
record signature, e.g. `mk : { name : String } -> Scroll ; mk h = … h.name
…` — failed with "cannot infer record type for field access `.name`". This
blocked the natural "map a helper over a list of host records" pattern and
forced helpers to take separate scalar arguments instead of one record. See
the "Record field access on parameters (row polymorphism)" entry in
`docs/TODO.md`.

`records-as-extensible-rows` had been listed as an explicit non-goal (design
§6.3 "Deferred", and the §12 divergence/non-goals table). It is now a goal:
the language is prioritizing this ergonomic gap over staying minimal on
record typing.

## Decision

Add **row polymorphism** to records (the Elm model). Records carry an
optional row-tail variable, so a record type is either:

- **Closed** — an exact field set (e.g. from a record literal
  `{ a = …, b = … }`), or
- **Open** — at-least-these-fields, with a row variable standing for "the
  rest" (produced by field access).

Field access is row-polymorphic: `.name : { r | name : a } -> a`. Record
literals are closed; `.field` demands an open record `{ field : a | ρ }`.

Unification does standard row unification:
- closed ~ closed requires an exact field match;
- open ~ closed requires the closed record to contain the open's fields and
  binds the row variable to the remainder;
- open ~ open merges via a common tail.

Row variables are kept distinct from ordinary type variables and are
quantified/instantiated by generalization, same as type variables.

This makes `\h -> h.name`, record-parameter helpers, and
`List.map (\h -> f h.name h.port) [ {name=…, port=…}, … ]` type-check.

## Alternatives considered

1. **Signature-directed parameter typing.** Push a decl signature's argument
   types into the body before inferring it. Fixes record-param helpers only
   *when* a signature is present, and does not make `\h -> h.name` work
   without one. Rejected as an incomplete subset of row polymorphism — kept
   as a mental fallback only if full rows had proved too large a lift.
2. **Status quo (scalar arguments).** Keep requiring helpers to spread
   record fields into positional scalar args. Rejected: this is exactly the
   ergonomic gap the decision closes.

## Consequences

- The natural record-map pattern works; the README's record-based example
  becomes real.
- Row-unification machinery (row variables, open/closed records) is added to
  the HM core.
- **Synergy:** this row machinery is most of what a principled *glyph
  pattern-matching* model needs (polymorphic variants), making ADR 0008
  cheaper to eventually implement.
- Cross-references design §4 (type representation) and the record-field-
  access entry in `docs/TODO.md`.
