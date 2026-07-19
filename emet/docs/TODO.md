# TODO / Backlog

Deferred work and known gaps, with pointers to the design/ADRs that explain the
context. These are intentional deferrals, not bugs.

Two backlogs: **A** is work inside the Emet language itself; **B** is the
Emet ↔ golem ecosystem integration, now that Emet is a subtree of the golem
monorepo. B's headline is model reconciliation.

## A. Emet language backlog

- **List patterns** (`case xs of [] -> … ; (x :: xs) -> …`). The main gap:
  without a way to destructure a `List`, self-recursion (ADR 0011) cannot walk
  one, so recursion over a fleet or a list of hosts is unwritable today. Highest
  value language item.

- **Mutual recursion for value decls.** Decls are inferred left-to-right, so
  only self-recursion works; two decls that reference each other do not. Needs
  the inferencer to group a strongly-connected component and generalize it
  together.

- **Full `appendable` `++`.** `++` is currently bound to `String.append` only
  (`String -> String -> String`). Elm's `++` is `appendable -> appendable ->
  appendable` (String *or* List). We deliberately did **not** add a third
  `appendable` constraint alongside `number`/`comparable` (ADR 0007); lists use
  `List.append` / `List.concat` for now. To generalize: either add an
  `appendable` bounded constraint (mirrors `number`/`comparable`) or make `++`
  a runtime-dispatching builtin with an inference rule that unifies both operands
  and the result to one String-or-List type. See ADR 0007.

- **One-line inline `case`.** `case … of` currently requires laid-out arms (each
  on its own line). A single-line `case x of A -> a` inside a larger expression is
  deferred because it needs a `parse-error(t)`-style layout close, the same
  mechanism `let … in` uses (ADR 0001 / design §8.1).

- **User-facing `type` declarations.** `Maybe`/`Bool`/`Order` are currently
  built-in sum types injected programmatically via the prelude constructor
  registry. The general `type Foo a = ...` declaration syntax (which would make
  these ordinary library types and let users define their own) is designed
  (design §6) but not yet parsed.

- **Skolem-escape check for over-general signatures.** Signature checking uses
  instantiate-and-unify (design §4.3 / ADR 0003), which accepts an *over-general*
  signature (e.g. `f : a -> a` on a monomorphic body) without error — it does not
  grant false polymorphism (we generalize the inferred type, so it's sound), it
  just fails to *reject* the bogus signature. Add proper skolemization + escape
  checking to reject these.

- **Glyph pattern-matching.** Glyphs are currently non-matchable; the
  concrete-subtype + permissive-injection model (ADR 0002) is sound only while
  nothing eliminates a glyph. The row machinery from ADR 0010 (row-polymorphic
  records) makes this cheaper to add principally. Matching on glyphs is deferred
  and kept open — the `Type` foundation is variant-ready so a principled model
  (polymorphic/row variants or nominal subsumption) can be added additively.
  See ADR 0008.

- **`emet-lsp` depth.** `emet-lsp` (`crates/emet-lsp/`) is diagnostics-only
  today. Add hover (inferred types), completion, and go-to-definition. These need
  the compiler to expose position-indexed type/scope information the LSP can query.

### Diagnostics / tooling

- **Multi-error parse recovery + rich CLI rendering.** The chumsky parser is set
  up with `Rich` errors, but `compile()` surfaces only the first error. Wiring
  true multi-error recovery needs the `Error` model + `main.rs` reworked to carry
  and render multiple diagnostics.

### Cleanup

- **Remove the transitional `Str` / `Glyphs` aliases.** `String` and `List Glyph`
  are canonical; `Str` and `Glyphs` are accepted as one-migration-step aliases
  (ADR 0003). Remove them once all examples/tests use the canonical spellings.

- **Row polymorphism for record-parameter field access — DONE (design), see
  ADR 0010.** Field access needs a concrete record at the access site, so
  `\h -> h.name` and record-parameter helpers fail with "cannot infer record
  type for field access `.name`" (a record-typed signature does not rescue it
  either). ADR 0010 decides the fix: full row polymorphism (open/closed records,
  row unification), making `.name` row-polymorphic even without a signature (the
  Elm behaviour). The code change lands separately from the ADR.

## B. Emet ↔ golem integration backlog

Emet now lives in the golem monorepo (`emet/`, crates under `crates/emet/` and
`crates/emet-lsp/`). This backlog is the work to make Emet golem's authoring
language rather than a standalone project.

- **Model reconciliation (headline).** Emet and golem describe desired state in
  two different vocabularies and must be reconciled before a compiled fleet can
  feed golem.
  - golem's model (golem root `CLAUDE.md`; types in `crates/golem-types/`, the
    source of truth): `Blueprint { name, packages }` → `State` / `Revision` /
    `Action`, packages-only bookkeeping today.
  - Emet's model: a program evaluates to `main : List Scroll` (a per-host fleet),
    one `Scroll` per host, each a list of glyphs across four kinds — `aptPackage`,
    `systemdService`, `file`, `lineInFile` (ADR 0002 / ADR 0009).
  - Decide the shared types (golem's `crates/golem-types/` is the source of
    truth) and how a compiled Emet fleet maps onto what `golemd` consumes —
    which Emet glyph kinds correspond to golem items, and how a `Scroll` relates
    to a golem host/blueprint. This gates every other integration item.

- **Binary wire format — implement ADR 0012.** Emet's content-addressed binary
  scroll output IS golem's stated plan-of-record wire format, the one that
  replaces "JSON exported from Nickel" (golem root `CLAUDE.md`). Implement the
  manifest of content-addressed scrolls per ADR 0012. golem's workspace already
  provides `serde`, `blake3`, `sha2`, `ed25519-dalek`, and `hex` — reuse them
  rather than re-picking crates. Coordinate the schema with `golem-types` /
  `golemd`; the artifact is a cross-repo contract versioned by `format_version`.

- **Emet supersedes Nickel as the authoring language.** golem authors state in
  Nickel today (`nickel/lib.ncl`). Emet is the intended replacement authoring
  surface. Plan the transition and eventual removal of the Nickel layer, gated
  on model reconciliation and the binary wire format above.

- **golem crates as flake outputs.** Add golem's own crates (`golemd` /
  `golemctl`) as `flake.nix` outputs. Needs their static-build specifics worked
  out.

- **Unify the docs sites.** Fold Emet's markdown docs into golem's
  Astro/Starlight docs site; today Emet's docs are a separate subtree.

- **Retire the standalone `golem-lang` repo.** Once golem is verified to build
  and pass with Emet embedded, retire the standalone `golem-lang` source repo and
  clean up the on-disk copy.

- **Publishing.** Decide and set up how Emet (and/or the wider golem toolchain)
  is published.
