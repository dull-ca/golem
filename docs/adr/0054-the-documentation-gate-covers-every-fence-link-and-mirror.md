# 0054 — The documentation gate covers every fence, link, and mirror

## Status

Accepted 2026-08-09 (decision by Dr. Dub). Extends ADR 0043, whose tree,
components, and goldens stand unchanged, and supersedes its final consequence —
the migration allowance that left a literal fence ungoverned. Runs inside ADR
0035's gate (`nix flake check`).

## Context

ADR 0043 moved the site's programs into `sites/website/examples/` as real files
and had CI compile them. It left one way out on purpose: "a page keeps working
with a literal fence until it is converted; the checker only governs what has
been converted, so partial adoption is honest rather than a lie of omission."

Eight days on, the way out held most of the code. 93 Emet blocks are fenced
across the two prose trees against 21 files in the examples directory, and a
sweep of them found nine `Quadlet.Workload` literals three fields behind the
library they name. That is the defect ADR 0043 exists for, sitting in the one
place ADR 0043 does not look — and it is not an argument against 0043's tree,
which is doing its job, but against the exemption.

Two further classes of rot surfaced in the same sweep, neither of them a
compiler's problem:

- A link to a route the site no longer publishes, or an anchor no heading
  produces. Both render, and both fail only for the reader.
- A page carrying a *rearranged* rendition of a real fleet program under the
  repo-root `examples/` — declarations reordered so a region can skip what the
  lesson is not about, the header and design comments dropped. A `.emet-ref`
  shows a program verbatim and cannot express this, so the two copies drifted
  with nothing comparing them.

Four constraints shape what a wider gate may do:

- `docs/adr/` and `docs/design/` describe the tree as it stood on the day of a
  decision. Holding them to today's paths would mean editing accepted records.
- Some blocks genuinely are not programs — one expression, one signature, one
  field of a record. A gate with no opt-out gets an opt-out invented for it, off
  to one side, uncounted.
- An opt-out that costs nothing becomes the way to make the gate quiet.
- A prose edit must not rebuild the release binaries. The flake's Rust source is
  an allow-list precisely so it does not.

## Decision

The gate covers every Emet block, every link, and every mirrored program, across
both prose trees — `docs/guide/` and `sites/website/src/content/docs/`. Three
workspace tests carry it (`apps/emet/tests/docs_{examples,fences,links}.rs`,
over a shared `docs_gate`), on the same terms ADR 0043 set: in-process against
the compiler's own API, under `cargo test`, and therefore gated by construction.

- **Every fence compiles, against the library golem ships.** Resolution runs
  through the repository's real `lib/` rather than a stub, so a page writing a
  `Quadlet.Workload` literal is held to the record `emetc` would resolve. `emet`
  and `elm` are two spellings of one language: the site has its own grammar,
  and `docs/guide/` is read through GitHub, which highlights `elm` and has never
  heard of `emet`. A fence that declares no `main` is given one, so a page may
  teach a signature or a helper without boilerplate around it.
- **`fragment` is the only opt-out, and it must be earned.** A block that is not
  a program says `` ```emet fragment `` and is skipped — and a second test
  compiles the marked fences and **fails the ones that succeed**. The marker is
  therefore a claim the build audits, not a way to go green.
- **Links, anchors, routes, and mentioned repository paths resolve.** Relative
  targets must exist; a site-absolute route must be a page the site publishes;
  an `#anchor` must name a heading, slugged the way Astro slugs it, so an anchor
  that passes is one a reader lands on; a backticked path whose first segment is
  a top-level directory of this repository must be a file it has; and no link
  points at `codeberg.org` (ADR 0035). Dated records are exempt from the
  path check, and `docs/superpowers/` — finished implementation plans — is out
  of the link check entirely.
- **A rendition declares what it mirrors.** A `.mirrors` sidecar beside an
  example names the real program it stands for. Both are compiled and their
  rendered plans compared: formatting, comments, and declaration order are free
  to differ, the plan is not.
- **The prose is its own build input.** The flake gains `documentedSource`
  beside `rustSource`, so only the test derivation sees a documentation change;
  the four binaries and the resolved dependency closure stay on the narrow
  source and stay cached.

ADR 0043's examples tree, `<Snippet>`, `<Output>`, `.expected-error`, and
goldens are unchanged and remain the way to show a program a reader can run.

## Consequences

- The `Quadlet.Workload` class of defect is closed on both surfaces rather than
  one. 54 of the 93 fences now compile on every `cargo test`; the other 39 are
  audited fragments.
- A fence becomes a legitimate resting place for code, which under ADR 0043
  alone it was not. The choice between a `<Snippet>` and a fence is now about
  what the reader gets — a file they can run, an asserted output, one source two
  pages share — and no longer about which one is safe.
  `sites/website/examples/README.md` is where an author is told that.
- The gate still cannot see whether the prose *about* a block describes what the
  block does, nor whether a `#region` still contains what the page claims. ADR
  0043's open seam is unchanged: prose about code remains a human problem.
- One fence carries `fragment` on mechanical truth rather than meaning. The
  `Secretspec.get` program in `docs/guide/reference.md` is complete and correct
  and fails only because a test has no provider and no fleet key (ADR 0047). One
  instance does not earn a second marker vocabulary; a second complete
  secret-bearing example is the trigger to add one that says *this needs
  secrets* instead of borrowing the one that means *this is not a program*.
- The anchor check reimplements `@astrojs/markdown-remark`'s slug rule.
  Upgrading Astro can move it, and a divergence shows up as an anchor the gate
  calls broken while the site serves it, or the reverse.
- Cost: three test binaries instead of one, a second source filter in the flake,
  and 54 compiles on every workspace test run.
