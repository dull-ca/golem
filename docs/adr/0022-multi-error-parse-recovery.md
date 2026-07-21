# 0022-multi-error-parse-recovery

## Status

Accepted 2026-07-21; implementation landed.

## Context

The compiler pipeline (ADR 0001) settled on chumsky 0.10 with `Rich` errors
precisely so the parser could report *multiple* errors per compile and recover
past a mistake — that was the diagnostics payoff that justified taking the
dependency. But the surface never used it. `parser::parse` collected whatever
`Rich` errors chumsky produced, yet `parse_source` / `compile` immediately did
`errors.remove(0)` and threw the rest away, returning a single `emet::Error`.
`main.rs` rendered exactly one ariadne report. The `Error` model was
single-valued end to end (`Result<_, Error>`), so even though chumsky was ready
to recover, nothing downstream could carry more than one diagnostic.

`docs/TODO.md` §A ("Diagnostics / tooling") names the work: wire true
multi-error recovery so one run reports several parse errors, and rework the
`Error` model plus `main.rs` to carry and render all of them. ADR 0018 already
gave the LSP a `Vec<Error>` diagnostics channel (`analyze_source` /
`analyze_project`), so any list the compiler produces should flow there too and
the editor should light up every parse error at once.

## Decision

**Recover in the parser at declaration boundaries; carry diagnostics as a list
through the whole surface; keep first-error wrappers for existing callers.**

### Parser recovery

The layout pass already inserts a virtual `;` (`Tok::VSemi`) between top-level
declarations. That token is the natural synchronization point. The top-level
`item` parser in `module_parser` gains

```
.recover_with(skip_until(any().ignored(), just(Tok::VSemi).ignored(), || TopItem::Recovered))
```

On a parse failure, recovery skips tokens until the next `VSemi`, emits the
`Rich` error, and yields a `TopItem::Recovered` sentinel. The enclosing
`.repeated()` then starts a fresh `item` at the following declaration — which,
if *also* malformed, fails and recovers again, so two independent bad decls
produce two errors. `parser::parse` drops the `Recovered` sentinels and keeps
the real items, returning `Vec<ParseError>`.

### Diagnostics as a list

A new list-carrying surface runs alongside the existing one:

- `parse_source_multi(&str) -> Result<Module, Vec<Error>>`
- `compile_all(&str) -> Result<Compiled, Vec<Error>>`
- `compile_file_all(&Path) -> Result<Compiled, Vec<Error>>`

`resolve::compile_entry` and `load_graph` now return `Vec<Error>`, so a
multi-module build reports every parse error in the offending file (each module
is parsed through the recovering path). `main.rs` renders all of them —
`report_errors` calls the existing single-error `report_error` once per
diagnostic, so each error keeps its own ariadne report and span.

`parse_source` / `compile` / `compile_file` stay as thin `errors.remove(0)`
wrappers over the `_multi` / `_all` variants, so every existing caller and test
(most of the suite asserts one `Error`) is unchanged.

### Parse-vs-type boundary (the honest scope)

Recovery is a **parse-phase** feature only. Type checking and evaluation stay
first-error: inference is sequential (Algorithm W over dependency-ordered SCCs;
ADR 0011) and unwinds at the first failure, and multi-*type*-error recovery
would mean fabricating types for un-inferable holes and risking spurious
cascades or corrupted exhaustiveness results. So a `Vec<Error>` from
`compile_all` holds **either** several `Phase::Parse` errors **or** a single
later-phase error — never a mix. Lex and header errors are likewise fatal and
single: there is nothing coherent to recover past before layout has run.

### Alternatives considered

1. **Make `compile` itself return `Vec<Error>`.** Rejected: it churns nearly
   the whole test suite and every caller for no gain over a first-error wrapper.
   The list lives on the `_all` / `_multi` names; the old names stay.
2. **`skip_then_retry_until` instead of `skip_until`.** Rejected: that strategy
   accepts a recovered item only when the *retry* re-parses cleanly, so two
   consecutive malformed decls collapse to one error. `skip_until` terminates
   each bad decl at its boundary as its own recovered item, so `repeated`
   restarts and the next bad decl reports too.
3. **Recover inside the `let`-block `decls_parser` as well.** Rejected for now:
   the `let … in` block relies on the ADR 0001 close-on-`in` handshake, where
   layout splices a virtual `}` when the parser is stuck on `in`. A skip-based
   recovery in that context could consume the `in` or the virtual brace the
   handshake depends on. Top-level recovery is where the TODO's target
   (multiple malformed decls) lives; the `let` interior stays first-error.

## Consequences

- One compile run reports every independent top-level parse error, each with a
  correct span, rendered as a separate ariadne report by `emetc` and as a
  separate LSP diagnostic by `emet-lsp` (which needed no change — it already
  consumed `analyze_source().diagnostics`).
- **Cascade is bounded, not eliminated.** Sync on `VSemi` means recovery is only
  as clean as the layout boundaries. An error whose bad tokens straddle a
  boundary, or a decl so malformed that layout misplaces its `;`, can still
  produce a follow-on error on what a human reads as the same mistake. The sync
  point keeps a *valid* decl after a bad one clean (a covered test), which is the
  property that matters; perfect minimality is not promised.
- The `parse-error(t)` / close-on-`in` handshake (ADR 0001) is untouched:
  recovery is scoped to the top-level `item` and never runs inside a `let`
  block, and `tests/layout.rs` / `tests/pipeline.rs` are byte-for-byte green.
- Two error surfaces now coexist (`compile` vs `compile_all`). The first-error
  wrappers are the compatibility shim; new tooling should prefer the `_all` /
  `_multi` variants.
- Type/eval diagnostics remain first-error. If sequential inference is ever
  reworked to accumulate independent type errors, this ADR's boundary is the
  place that changes.
