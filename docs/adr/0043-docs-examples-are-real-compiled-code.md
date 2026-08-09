# 0043 — Documentation examples are real code, compiled and asserted in CI

## Status

Accepted 2026-08-01 (decision by Dr. Dub). Bears on `sites/website/` and on
ADR 0035 (`nix flake check` is the whole CI gate).

Extended by ADR 0054 (2026-08-09), which widens the gate to every fence in both
prose trees, to links and anchors, and to rearranged renditions of real fleet
programs. Everything decided below stands except the last consequence: the
migration allowance is spent, and a literal fence is now governed rather than
merely tolerated until conversion.

## Context

A four-axis review of the docs site on 2026-08-01 compiled every Emet
snippet it published. Three did not compile at all — a type that does not
exist (`File` for `Filesystem`), a signature that cannot widen
(`-> Glyph` where the constructor yields `LineInFile`), and a currying
example whose function name shadowed the builtin `+` desugars to, so it
recursed until the evaluator gave up. Every one of them was on a reader's
happy path. Separately, all seven `--text` output blocks omitted the first
line the compiler actually prints.

None of this was carelessness: the snippets were correct when written and
the language moved. Prose duplicated into a page cannot be re-checked by
the thing it describes, so it decays silently and is only discovered by a
reader who types it in. The pattern that solves it is old and proven (the
Go book's examples are pulled from compiled source, never retyped): the
documentation must not *contain* code, it must *reference* code that the
build already compiles.

## Decision

Documentation examples are files in a docs-owned tree,
`sites/website/examples/`, compiled by CI. Pages reference them; pages
never carry a copy.

- **Snippets.** A build-time `<Snippet file="…" region="…"/>` component
  reads the file and renders the named region (delimited by `-- #region
  <name>` / `-- #endregion` comments, which the compiler ignores). A page
  cannot drift from the file because it holds no copy of it.
- **Expected failures are first-class.** An example that must fail carries
  its expected diagnostic beside it (`<name>.expected-error`). CI asserts
  the compiler still produces that error — so a broken example teaching a
  real mistake stays correct, and a *silently fixed* language footgun
  surfaces as a CI failure rather than a stale lesson.
- **Output blocks are generated and asserted.** For every example with a
  recorded output, the real renderer produces the text and it is compared
  against a checked-in `.golden` file the page includes; drift fails.
  Regeneration is the same code path with a flag
  (`UPDATE_DOCS_GOLDEN=1`), so a deliberate change is one command and an
  accidental one is a failure. A page's output block is therefore the
  tool's own output, not a transcription of it.
- **The checker is a test, not a script.** It lives in the workspace test
  suite and runs under `cargo test`, in-process against the compiler's own
  API rather than by shelling out — fast enough to run constantly, and
  therefore run. Each example is its own assertion, and a failure names
  the file and prints the diagnostic. `nix flake check` (ADR 0035) already
  runs the workspace tests, so this is gated by construction rather than
  by a CI step anyone could forget to add.

Real fleet programs under `examples/` stay where they are and may also be
referenced by region where a page wants the genuine article; the docs tree
exists for programs written to teach, including the ones that must fail.

## Consequences

- A language or format change that invalidates a documented example breaks
  CI on the commit that makes it, naming the file. The class of defect
  this ADR exists for cannot reach a reader.
- Every example is a real program, so an example can be run, and a reader
  can be pointed at the file rather than told to reassemble a page.
- Cost: two artifacts per example (source, and a golden where output is
  shown), a component between the author and the page, and a CI job that
  needs the toolchain — which `nix flake check` already has.
- Regions are the seam that can still rot: renaming a region breaks the
  build (good), but a region that no longer contains what the prose claims
  is invisible to the checker. Prose about code remains a human problem.
- Migration is incremental. A page keeps working with a literal fence
  until it is converted; the checker only governs what has been converted,
  so partial adoption is honest rather than a lie of omission.
