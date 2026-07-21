# 0016-emet-module-system-elm-modeled

## Status

Accepted.

## Context

Goal #3 of the golem refactor is to re-author the lichess examples
(`examples/lichess/*.ncl`) in Emet, implementing the higher-level abstractions —
today Nickel's `contrib` (workload, postgres, worker), `firewall`/ingress, and
the cross-host `ref` helpers (`nickel/lib.ncl`, `nickel/contrib.ncl`) — *in the
Emet language itself*, so they compile down to the four glyphs golemd now enacts
(ADR 0014/0015). Two questions fall out:

1. **Does re-authoring need a module system at all?** Emet today is a single
   file: top-level decls, one `main : List Scroll`, module-*qualified* built-ins
   (`List.map`, `String.join` — ADR 0006) but no user modules, no `import`, no
   `exposing`. The reference for every Emet design decision is **Elm**
   (`emet/CLAUDE.md`).
2. **What does Emet still lack that the lichess abstractions need?** The headline
   gap (`emet/docs/TODO.md` §A) is **list patterns** (`case xs of [] -> … ;
   (x :: xs) -> …`): without list destructuring, self-recursion (ADR 0011) cannot
   walk a list, so recursion *over a fleet or host list* is unwritable. Also open:
   mutual recursion, user-facing `type` declarations (partially — `roles.emet`
   shows `type Role = Web | Db` working), and full `appendable ++`.

Reading the current lichess Nickel (`manta.ncl`, `orbit.ncl`, `scaly.ncl`, …):
each host is a flat record of `workloads`/`services`/`ingress` lists. The
abstractions are shallow — a "service" is a container + a systemd unit + a
firewall opening; a "workload" is a container with no ingress. Crucially the one
genuinely hard piece is **cross-host reference resolution** (`ref.service`,
`resolve.ncl`): an env value on one host that names a service on another,
substituted at translation time. That is a *fleet-global* computation, not a
per-host one.

The examples do **not** obviously need recursion over unbounded lists: the host
set is small and enumerated, and `List.map`/record-parameter helpers
(`record-hosts.emet`, `roles.emet`, `app-helper.emet`) already express
"map an abstraction over a list of host records." What they need is (a) a place
to *put* the shared abstractions so several host files can reuse them, and (b) a
way to express the higher-level shapes and lower them to glyphs.

## Decision

### 1. A module system IS warranted — but a small, Elm-shaped one, and it is not on the critical path for a first lichess port

Re-authoring lichess is achievable with today's language **plus multi-file
`import`**, because the abstractions are `List.map`-shaped, not recursion-over-
list-shaped. List patterns are **not** a blocker for the lichess port and are
sequenced *after* it (`PLAN.md`). The module system's job here is **code
organization and reuse across host files** — a shared `Lichess` library of
abstractions imported by each host — not new computational power.

So: build a **minimal Elm-modeled module system** whose scope is exactly
namespacing + import, deferring Elm's heavier machinery.

### 2. The Elm-modeled design (what we build)

Mimic Elm (`emet/CLAUDE.md` mandate) at the surface:

- **`module Name exposing (..)` / `exposing (a, b, Type(..))`** header, one
  module per file, file path = module name (Elm's rule). A module is a set of
  top-level decls (the thing Emet already parses) with an exposing list gating
  visibility.
- **`import Foo` / `import Foo as F` / `import Foo exposing (bar)`.** Qualified
  access `Foo.bar` reuses the **exact dotted-name resolution Emet already has**
  for built-ins (`List.map`, `String.join` — ADR 0006); a user module `Foo`
  resolves `Foo.bar` the same way the prelude resolves `List.map`. This is the
  key leverage: the qualified-name machinery exists, so user modules extend it
  rather than inventing a new resolution path.
- **The `main : List Scroll` bottom is unchanged.** Exactly one module in a
  compilation is the entry (has `main`); the rest are libraries. The single-`main`
  model (ADR 0009) is preserved — modules add *namespacing*, not multiple entry
  points.
- **Name resolution**: an import graph over files, resolved before inference; no
  cycles in the first cut (Elm forbids import cycles too). Each module is
  type-checked against the interfaces (exposed decls + their inferred/annotated
  types) of what it imports.

### 3. What we deliberately DEFER from Elm (and why it is safe to)

- **User-defined `type` in libraries at full generality.** `type Role = Web | Db`
  already works enough for `roles.emet`; the general `type Foo a = …` is a
  separate TODO (`emet/docs/TODO.md` §A / design §6) and is only needed if a
  lichess abstraction introduces its own sum type. First cut: lean on records +
  the built-in sum types; add general `type` when an abstraction demands it.

  **As implemented:** user `type` declarations — nullary sum types
  (`type Role = Web | Db`), record-carrying types, *and* parameterized
  `type Foo a = …` — all work and **cross module boundaries** (constructor
  schemes generalized over the params, the type constructor registered at its
  arity, arity>0 types importable). Two related pieces from the module work,
  both now landed: **imported types are usable in annotations** in the importing
  module (`imported_types` threaded through `check_entry`/`check_library`); and
  **importer-side pattern-matching on `Type(..)` constructors** now resolves —
  `case x of Ctor -> …` on a constructor from another module type-checks, and
  the exhaustiveness checker sees the imported type's full constructor set
  (`import_constructors` / `seed_imported_constructors`). A type exposed without
  `(..)` stays unmatchable in importers.
- **List patterns / recursion over host lists.** Not needed for the lichess port
  (§1). Sequenced after, as its own language-backlog item — it unblocks *future*
  fleet-wide recursion (e.g. "N numbered nodes" beyond `numbered-nodes.emet`'s
  `List.append` trick), not this port.
- **Exposed-type opacity, port modules, effect modules, package publishing.**
  All Elm features with no analogue in Emet's inert-IR world. Out of scope.

### 4. The one abstraction that needs real design: cross-host references

The lichess `ref.service`/`resolve` mechanism (an env value on host A naming a
service on host B, substituted fleet-wide) is the only lichess feature not
expressible as a per-host `List.map`. Two options, [RATIFY] the first:

- **(Recommended) Express it as ordinary Emet values, no new language feature.**
  Because Emet is a *program*, the fleet can be built from a shared
  `let`/record of host facts (names, ports) that every host expression reads —
  cross-host references become ordinary value references in one program, resolved
  by evaluation, not by a placeholder-substitution pass. This is strictly more
  principled than Nickel's string-placeholder `resolve.ncl` and needs **zero**
  new language machinery — it is what a typed functional language buys us. The
  module system just lets that shared fact-table live in an imported `Fleet`
  module.
- **(Fallback) A `ref`-style helper returning a String**, mirroring Nickel, if
  the value-level approach proves awkward for a specific reference. Kept as a
  library function, not a language feature.

## Alternatives considered

1. **No module system; re-author lichess as one big Emet file (or copy-paste
   helpers per host file).** Rejected: the lichess fleet is ~7 hosts sharing the
   same handful of abstractions; without import, either everything lives in one
   unwieldy file or the abstractions are duplicated per host. A minimal import
   system is the smallest thing that gives reuse. (But note: a *single-file*
   first port is a viable **Phase-0 spike** to de-risk the abstractions before the
   module system lands — see `PLAN.md`.)
2. **Build the full Elm module system now (opaque types, cycles via SCC, package
   manager).** Rejected: far more than the lichess port needs; violates YAGNI and
   Emet's small-footprint value. Namespacing + import + qualified access is the
   90% that matters; the rest is added when a concrete need appears.
3. **Block the lichess port on list patterns first.** Rejected: analysis of the
   examples shows they are `List.map`-over-enumerated-hosts shaped, not
   recursion-over-unbounded-list shaped. Coupling the port to the hardest language
   item would stall goal #3 behind unrelated work. List patterns remain the top
   language-backlog item, sequenced independently.
4. **Keep Nickel's placeholder-string reference model, ported literally.**
   Rejected: string placeholders + a substitution pass are exactly the
   templating Emet's design rejects (ADR 0004). Cross-host refs as ordinary values
   in one program are more honest and need no new feature.

## Consequences

- **A minimal, Elm-shaped module system is added to Emet**: `module … exposing`,
  `import … [as …] [exposing …]`, file-path = module-name, qualified access
  reusing the ADR 0006 dotted-name resolution, single-`main` entry preserved. It
  touches the lexer (keywords `module`/`import`/`exposing`/`as`), parser (headers),
  and a new pre-inference **name-resolution / import-graph** stage; inference and
  eval work per-module against imported interfaces.
- **List patterns are explicitly NOT required for the lichess port** and stay the
  top independent language-backlog item; this ADR removes them from goal #3's
  critical path and records why.
- **Cross-host references become ordinary Emet values** (recommended), replacing
  Nickel's placeholder-substitution `resolve.ncl` with evaluation — no templating,
  consistent with ADR 0004.
- **A single-file first port is available as a de-risking spike** before the
  module system lands, keeping goal #3 unblocked while the module work proceeds in
  parallel.
- **Deferred Elm features are named** (general `type`, opacity, port/effect
  modules, publishing) so the boundary of "Elm-modeled" is explicit and additive.
- **Cross-references:** extends the module-qualified-builtins resolution (ADR
  0006); preserves the single-`main`/`Scroll` output bottom (ADR 0009); the
  abstractions it hosts compile to the four glyphs (ADR 0002) that golemd enacts
  (ADR 0014/0015); interacts with deferred list patterns and user `type` decls
  (`emet/docs/TODO.md` §A).
