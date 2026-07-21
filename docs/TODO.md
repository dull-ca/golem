# TODO / Backlog

Deferred work and known gaps, with pointers to the design/ADRs that explain the
context. These are intentional deferrals, not bugs.

Two backlogs: **A** is work inside the Emet language itself; **B** is the
Emet ↔ golem ecosystem integration, now that Emet lives in the golem
monorepo. B's headline is model reconciliation.

## A. Emet language backlog

- **List patterns — DONE.** `case xs of [] -> … ; (x :: xs) -> …` now
  destructures a `List`: `::` is a right-associative level-5 operator (→ the
  `cons` builtin) and `[]` / `(head :: tail)` / `[a, b]` are patterns. This
  closes the self-recursion gap (ADR 0011) — recursion can now walk a list of
  hosts or glyphs. Exhaustiveness/redundancy covers lists by modeling `List` as
  a synthetic two-constructor (`[]`/`::`) sum through the existing Maranget
  checker (ADR 0005).

- **Mutual recursion for value decls.** Decls are inferred left-to-right, so
  only self-recursion works; two decls that reference each other do not. Needs
  the inferencer to group a strongly-connected component and generalize it
  together.

- **Full `appendable` `++` — DONE.** `++` is now `appendable -> appendable ->
  appendable`, satisfied by `String` and `List a` only, mirroring Elm exactly.
  `appendable` joins `number`/`comparable` as a third `Constraint` bounded type
  variable (ADR 0007), threaded through `bind`/`unify`/`merge_constraints`/
  `constraint_admits` in `infer.rs`; `merge_constraints` now rejects the
  unsatisfiable merges (`appendable` shares no type with `number`/`comparable`).
  `++` desugars to a single `append` prelude builtin carrying the `appendable`
  scheme, which dispatches at eval time to string or list concatenation on the
  runtime value. `List.append` / `List.concat` remain for explicit use;
  interpolation still desugars to `String.concat`.

- **One-line inline `case`.** `case … of` currently requires laid-out arms (each
  on its own line). A single-line `case x of A -> a` inside a larger expression is
  deferred because it needs a `parse-error(t)`-style layout close, the same
  mechanism `let … in` uses (ADR 0001 / design §8.1).

- **User-facing `type` declarations.** Non-parameterized user types work now:
  single-constructor record-carrying types and nullary sum types (e.g. `type
  Role = Web | Db`) parse, infer, and cross module boundaries. The general
  *parameterized* `type Foo a = ...` (design §6) remains deferred.

- **Imported value constructors in scope for pattern-matching — DONE.** A
  type's constructors imported via `exposing (Type(..))` now resolve in
  `case x of Ctor -> …` on the importer side, and the exhaustiveness/redundancy
  checker sees the imported type's complete constructor set across the module
  boundary. The resolver carries each open-exposed type's constructor schemes
  and full variant set on the `Interface` and threads them into inference
  (`ImportedConstructors`), the pattern-side counterpart to the imported-type
  arities used for annotations. A type imported without `(..)` still keeps its
  constructors invisible to the importer (ADR 0016 §3).

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

- **`emet-lsp` depth.** `emet-lsp` (`apps/emet-lsp/`) is diagnostics-only
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

Emet now lives in the golem monorepo (`apps/emet` and `apps/emet-lsp`). This
backlog is the work to make Emet golem's authoring language rather than a
standalone project.

- **Model reconciliation (headline) — DONE (ADR 0014).** The two vocabularies
  were reconciled by *retiring* golem's rich model rather than mapping onto it.
  golem's `Blueprint`/`State`/`Action` model was deleted; the compiled `Scroll`
  *is* the desired state. The shared model now lives in the `scroll-format`
  crate, and `golemd` diffs a manifest's scroll by content id and enacts the
  four glyphs. Emet's model — `main : List Scroll`, one `Scroll` per host, each
  a list of `aptPackage`/`systemdService`/`file`/`lineInFile` glyphs (ADR 0002 /
  ADR 0009) — is authoritative.

- **Binary wire format — DONE (ADR 0012/0013).** `emetc`'s default output is the
  content-addressed binary manifest (BLAKE3 over postcard, per-scroll and
  per-glyph content ids, `format_version`), defined in the shared `scroll-format`
  crate that both `emetc` (writer) and `golemd` (reader) depend on. `--text` and
  `--json` are the human/debug views.

- **golemd glyph rewrite — DONE (ADR 0014/0015).** `golemd` ingests the manifest,
  selects its host's scroll, diffs by content id (`GlyphOp` Install/Remove/
  Replace/Noop), and enacts through reversible `Reconciler`s (`apply`/`reverse`
  with journalled `Inverse`), collapsing revisions to `Init`/`Reconcile`.

- **Emet supersedes Nickel as the authoring language — DONE (ADR 0016).** Emet
  gained a minimal Elm-shaped module system (`module … exposing`, `import`), the
  lichess examples were re-authored in Emet (`examples/lichess/*.emet`), and the
  Nickel layer (`nickel/`, the `.ncl` examples, and golemctl's `nickel export`
  shell-out) was retired.

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
