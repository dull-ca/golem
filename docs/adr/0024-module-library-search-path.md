# 0024-module-library-search-path

## Status

Accepted (2026-07-22).

## Context

The Elm-modeled module system (ADR 0016) resolves `import Foo` strictly to
`<entry-dir>/Foo.emet`: `resolve::load_graph` joins the importer's module name
onto the entry file's own directory and nowhere else. That is enough for a
single self-contained program, but the lichess re-authoring (goal #3 of the
golem refactor) puts one entry program per host directory
(`examples/registry/`, `examples/website/`, `examples/lichess/`) over a *shared*
library of abstractions — a `Quadlet` module, a `Fleet` fact table. With
entry-dir-only resolution, that shared library must be copied into every entry
directory, defeating the reuse the module system exists to provide.

Elm solves the same problem with `elm.json`'s `source-directories`: a list of
roots the compiler searches, in order, to resolve a module name to a file. Emet
wants the same capability with the least disruption to the callers that already
drive `compile_file`:

- `fleet apply` shells `emetc build <file>` (`apps/fleet/deploy.py`), passing
  only the entry path.
- The lichess integration test calls `compile_file(&entry)` with just a path.
- The LSP calls `analyze_project(entry)` with just a path.

None of these want to grow a new required argument, and the daemon side
(golemd, scroll-format) must not be touched at all — resolution is purely a
compile-time, writer-side concern.

Two mechanisms were weighed:

1. An **Elm-style manifest** (`emet.json` carrying `source-directories`),
   discovered by walking up from the entry file. No caller passes anything new;
   the library roots travel with the project on disk.
2. A repeatable **CLI `--lib <dir>`** on `emetc` plus an `EMET_PATH` env
   fallback, threaded into `compile_file` / `load_graph`. Every caller —
   `deploy.py`, both test harnesses, the LSP — must learn to pass it, and the
   library roots live outside the source tree in invocation flags.

## Decision

Adopt the **manifest**. Resolution of `import Foo` searches, in order:

1. the entry file's own directory (ADR 0016's behavior, unchanged); then
2. each directory named in the nearest `emet.json`'s `source-directories`, in
   listed order,

taking the **first** `<dir>/Foo.emet` that exists.

`emet.json` is discovered by walking up the ancestors of the entry file's
directory to the filesystem root and taking the first `emet.json` that parses;
its `source-directories` entries are resolved relative to the manifest's own
directory. A project with no `emet.json` keeps the exact ADR 0016 behavior —
entry-directory-only resolution — so single-file programs and the existing
fixtures are unaffected.

**Precedence is entry-directory-first.** When a module name exists both beside
the entry and in a library directory, the entry-directory file wins; among
library directories, earlier `source-directories` entries win. This makes a
project-local override of a library module a deliberate, predictable act rather
than an error.

The **canonical shared-library location** is a repo-root `emet.json` listing a
repo-root `lib/`:

```json
{ "source-directories": ["lib"] }
```

so any entry anywhere under the repo resolves `import Quadlet` to `lib/Quadlet.emet`.
Populating that `lib/` (moving the Quadlet library into it and re-expressing the
`examples/` entries against it) is deliberately left to the follow-up step; this
ADR lands only the mechanism and the convention.

Cycle rejection (ADR 0016) is unchanged and now spans directories: a library in
`lib/` importing a module back in the entry directory is loaded, ordered, and
cycle-checked exactly as an entry-dir import is, because the import graph is
keyed by module name over whatever files the search path resolved. The
single-`main` entry rule, `exposing`/visibility gating, and qualified access all
carry over untouched — the search path only changes *which file* a name resolves
to, not what happens after it is loaded.

The manifest is parsed with `serde` / `serde_json`, already workspace
dependencies (via `scroll-format`/postcard's ecosystem), so this adds no new
third-party crate to Emet's small footprint. JSON matches Elm's `elm.json`
precedent.

## Consequences

- **Zero caller churn.** `emetc build <file>`, `compile_file(entry)`,
  `analyze_project(entry)`, and `fleet apply` are unchanged; the library roots
  are discovered from disk. This is the decisive advantage over the CLI-flag
  option, which would have touched `deploy.py`, both test harnesses, and the LSP.
- **A shared library lives in one place.** Entries in different directories
  import it by name; the repo-root `emet.json` + `lib/` convention is the
  default the mechanism finds.
- **The LSP participates for free.** `analyze_project` routes through the same
  `resolve::load_graph`, so cross-file hover / go-to-definition (ADR 0018) work
  across the search path with no LSP-specific change.
- **Entry-dir precedence is a design choice, recorded here.** A name clash
  resolves to the entry-dir file, not an error; a project can shadow a library
  module locally. The alternative (clash = error) was rejected as more hostile
  to the common "override one module" case.
- **The manifest is discovered, not passed.** An entry compiled from a directory
  with no ancestor `emet.json` sees only its own directory — the same as before
  this ADR — so nothing silently changes for existing single-dir programs.
- **New at-rest input.** `emet.json` becomes a project file the compiler reads;
  a malformed or absent one degrades to entry-dir-only resolution rather than
  failing the build. It is writer-side only and never reaches golemd or the wire
  manifest.
- **Cross-references:** extends ADR 0016's import graph with a search path;
  preserves its cycle rejection, single-`main`, and visibility rules; serves the
  LSP resolution path (ADR 0018); has no effect on the binary manifest (ADR
  0012/0013) or golemd.
