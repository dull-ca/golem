# Golem — project conventions

## The model

You author a fleet in **Emet** (`apps/emet/`), a typed functional language. A
program evaluates to `main : List Scroll` — one **Scroll** per host. A Scroll is
a recursive, strict tree (ADR 0031): each level holds *either* **glyphs** (a
leaf unit) *or* named sub-scrolls (a branch), never both. Leaf units are the
failure-isolation boundary — one unit's failure never rolls back a sibling — and
each may carry an optional `policy`. The glyphs come over exactly four kinds:

- **`aptPackage { name }`** — a Debian package.
- **`systemdService { unit }`** — an enabled+started unit.
- **the filesystem glyph** — `file` / `directory` / `symlink`, three surface
  spellings of one `Glyph::Filesystem { path, entry }` whose `entry` is an
  `Entry` sum (`File { contents, perms }` | `Directory { perms }` | `Symlink
  { target }`) with typed `Perms { mode, owner, group }`. Each arm carries only
  its own fields, so illegal states (a symlink with a mode, a directory with
  contents) are unrepresentable (ADR 0019).
- **`lineInFile { path, line }`** — one line ensured present in a file.

There is no fifth resource kind. `directory` and `symlink` are **variants of the
filesystem entry**, not new glyphs — just as `AptPackage` | `SystemdService` are
variants of one `Glyph` sum — so the count stays four *reconciler-owned kinds*.
Richer shapes (workloads, services, ingress) are Emet library abstractions that
*compile down* to these four glyphs — never new golemd kinds.

`emetc` compiles a program to a binary, content-addressed **manifest** (BLAKE3
over postcard bytes; per-scroll and per-glyph content ids). golemd ingests the
manifest, selects its host's scroll, and **diffs by content id** into glyph
operations (`GlyphOp` — `Install` / `Remove` / `Replace` / `Noop`, keyed by
`Glyph::key()`, versioned by content id). It enacts each op through a reversible
**Reconciler** (`apply` captures the prior state as an `Inverse`; `reverse`
restores it exactly) and journals the ordered outcomes as an append-only
**Revision** (`Init` / `Reconcile`) so upgrades and removals undo precisely
what golem did. golem only ever reverses edits it recorded — it never touches
pre-existing host state.

Source of truth for the wire model: the **`scroll-format`** crate
(`libs/scroll-format/`), shared by writer (`emetc`) and reader (`golemd`).
The authoring contract is the Emet language. Build & run: `QUICKSTART.md`.

## The wire format is an implementation detail

The model above is the contract between `emetc` and `golemd` — not the exact
bytes. The manifest is binary postcard today; its `format_version` guards
artifacts at rest. The model doesn't change, the serializer might. Don't
elevate encoding details (postcard field order, hex) as the headline — though
note that because the format is non-self-describing, a glyph's field/variant
order *is* the encoding, so reordering one is a `format_version` bump, not a
free refactor.

## Skill routing

Match a request to a skill → invoke it via the Skill tool. The correct
skills and agents are Dr. Dub's own `/lw:*` set — route work through them
(implementation → `lw:implementer`, prose → `lw:documenter`, decisions →
`lw:adr`, git → `lw:historian`, …), including when delegating to agents.
Don't run gstack ceremony (telemetry, gbrain, decision briefs, codex gates)
unless asked. Auto mode is on; prefer action over planning.

## Git

Commits are allowed without asking, provided the work runs through the
`/lw:*` skills — invoke `lw:historian` before touching git and follow it
(branching, commit curation, message style). Pushes, resets, and anything
that rewrites published history remain Dr. Dub's call.

## Docs

Two separate trees:

- **`sites/website/`** — the Astro/Starlight public docs site, on the current
  glyph/scroll/manifest model. Run and build from there (`sites/website`).
- **`docs/`** — the markdown design docs: `adr/`, `design/`, `guide/`,
  `PLAN.md`, `TODO.md`. The decision and backlog record, not the published site.
