# 0001-chumsky-parser-layout-owns-parse-error

## Status

Accepted

## Context

The compiler pipeline is `lexer.rs` (chars → tokens) → `layout.rs` (Haskell
2010 offside rule) → `parser.rs` (hand-written recursive descent) →
`infer.rs` (Algorithm W) → `eval.rs` → `ir.rs`.

Layout and the parser currently perform a live handshake to realise the
Haskell `parse-error(t)` layout clause: when the recursive-descent parser
hits `in` with a `let`-opened implicit block still open, it calls
`Layout::close_implicit()` to splice a virtual `}` mid-parse. This is what
lets single-line `let x = e in e` parse.

The team wants better parser diagnostics — multiple errors per compile and
error recovery — with rich spans to feed the existing `ariadne` renderer in
`main.rs`.

## Decision

Replace only the parser stage with `chumsky` 0.10, operating over the
laid-out token stream. The hand-written lexer and layout pass are unchanged.

`parse-error(t)` responsibility moves entirely into the layout pass, which
now emits a complete token vector — all virtual braces/semicolons resolved —
with no parser feedback loop. Concretely: a close-on-`in` rule. `in` is the
only `parse-error(t)` trigger in this grammar (the parser only ever closes
an implicit block on `in`), so this single rule fully replaces the
handshake. Implicit layout contexts are tagged with their origin so the
rule closes only a `let`-opened block, never the module-level implicit
block — the multi-line `let` case already closes via the normal dedent
rule.

### Alternatives considered

1. **Keep the hand-written recursive-descent parser.** Pro: `parse-error(t)`
   stays exact; no new dependency, in line with the repo's small
   dependency-footprint goal; the parser is only ~490 lines and works. Con:
   single-error, weaker diagnostics and no recovery.
2. **chumsky with a two-phase fallback** (parse, and on single-line-let
   failure, retry). Rejected: ugly, and defeats the point of a clean
   combinator parser.
3. **(Chosen) chumsky + layout-owns-parse-error(t).**

## Consequences

- Gains multi-error recovery and richer `ariadne` spans.
- Adds the `chumsky` dependency — mild tension with the repo's small
  dependency-footprint value; accepted for the diagnostics payoff.
- Trades the general `parse-error(t)` algorithm for a grammar-specific
  approximation. It is complete for today's grammar — only `let` opens
  implicit blocks; `where` and `case … of` are reserved but unused, and only
  `let` pairs with `in`. If `where` or `case … of` are added later, the
  layout close-rules must be extended accordingly. This is the main thing
  to remember about this decision.
- `tests/layout.rs` is unchanged and stays green: the multi-line `let`
  output is byte-for-byte identical; only the single-line case gains the
  close-on-`in` virtual `}`.
