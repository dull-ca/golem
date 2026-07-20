# Golem — project conventions

## The model

You author a fleet in **Emet** (`emet/`), a typed functional language. A
program evaluates to `main : List Scroll` — one **Scroll** per host, each a
list of **glyphs** over exactly four kinds:

- **`aptPackage { name }`** — a Debian package.
- **`systemdService { unit }`** — an enabled+started unit.
- **`file { path, contents, mode }`** — a file with concrete contents.
- **`lineInFile { path, line }`** — one line ensured present in a file.

There is no fifth resource kind. Richer shapes (workloads, services, ingress)
are Emet library abstractions that *compile down* to these four glyphs — never
new golemd kinds.

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

Match a request to a skill → invoke it via the Skill tool. Don't run gstack
ceremony (telemetry, gbrain, decision briefs, codex gates) unless asked.
Auto mode is on; prefer action over planning.

## Git

Never use git unless explicitly asked — no commits, pushes, branches, or
resets. The user decides when to commit.

## Docs

Two separate trees:

- **`sites/website/`** — the Astro/Starlight public docs site, on the current
  glyph/scroll/manifest model. Run and build from there (`sites/website`).
- **`docs/`** — the markdown design docs: `adr/`, `design/`, `guide/`,
  `PLAN.md`, `TODO.md`. The decision and backlog record, not the published site.
