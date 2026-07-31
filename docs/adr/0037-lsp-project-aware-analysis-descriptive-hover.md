# 0037 — LSP project-aware analysis and descriptive hover

## Status

Accepted 2026-07-31. Supersedes ADR 0018's description of the LSP surface,
which had gone stale on all counts; ADR 0018's decisions (QueryIndex in the
compiler, thin LSP presentation layer) are carried forward and extended,
not reversed.

## Context

ADR 0018 gave emet-lsp hover, completion, and goto-definition off a
compiler-owned `QueryIndex`, but every request analyzed the open document
in isolation: `analyze_source` fed the type checker empty import
environments. In practice (first exercised by
`examples/fishnet-farm/farm.emet`, which imports a *type*), that produced
false diagnostics on imported type constructors, aborted inference —
leaving the index sparse and hover null across the file — and masked a
reachable panic when a library failed its check. Hover also carried only
the rendered type: no documentation, no origin, and nothing at all on type
names, which have no expression site in the index. Dr. Dub's acceptance
bar, verbatim: hovering `Scroll` "should show me constructors and/or
docs".

## Decision

- **Analysis is project-aware.** The LSP analyzes the open document as the
  entry of its own import graph over the ADR 0024 search path, with the
  editor buffer overlaying its on-disk copy. Diagnostics, hover,
  completion, and goto all read the same analysis and must agree with
  `emetc` on a saved file. Pathless buffers degrade to the old
  single-file analysis rather than failing.
- **The index records more than expressions.** Type-name spans
  (annotations, declarations, exposing lists) and exposing-list value
  names carry the same `(span, Type)` / `(span, DefSite)` facts a use
  site gets, so hover and goto work anywhere a name appears.
- **Hover describes.** A hover payload is: the rendered signature or type
  declaration (custom sum types render their constructors); the
  contiguous `--` doc block above the definition, when one exists; and an
  origin line naming the module for imported symbols. The four
  parser-built authoring types (`Scroll`, `Policy`, `Contents`,
  `OnExhaust`) have no declaration data in the compiler, so a
  prelude-owned doc/shape table restates their authoring shape and
  meaning — hand-maintained against `parser.rs::build_constructor`,
  guarded by a NOTE and test anchors.
- **Document symbols.** `textDocument/documentSymbol` lists the buffer's
  top-level definitions with rendered types and accurate ranges.
- The LSP stays a thin presentation layer: span recording and doc
  extraction live in `apps/emet` beside the QueryIndex; emet-lsp renders.
  The synchronous re-analyze-per-request model stays (~70 ms on the
  largest example; a cache waits for evidence it is needed).

## Consequences

- Editor diagnostics are now trustworthy on multi-module fleets — the LSP
  and the compiler cannot disagree about whether a program is well-typed.
- Library documentation is leverage: a `--` block above a definition is
  now the hover text at every call site, which is why Quadlet's exported
  surface gained doc comments in the same change set.
- The builtin-type table is a hand-written restatement of parser
  behaviour; changing a builtin's field set without updating it produces
  wrong hover docs. The NOTE at the table and its test anchors are the
  guard.
- Known residual gaps, accepted for now: a cross-file *type* goto lands at
  the `type` keyword's column 0 rather than on the name; polymorphic
  imported values render internal type variables (`t3 -> t3`) instead of
  normalized ones; imported-module diagnostics surface only when that file
  is opened; the buffer overlay covers the single open document.
