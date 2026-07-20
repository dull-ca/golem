# 0013-implement-binary-manifest-and-scroll-format-crate

## Status

Accepted.

> This ADR realizes ADR 0012 (which is a plan). It records *where the code
> lands* and *how it is structured*; it is still a decision record, not the
> implementation. The implementation pass follows `PLAN.md` Phase 1.

## Context

ADR 0012 decided *what* `emetc` emits: a deterministic, binary,
content-addressed **Manifest** of **AddressedScroll**s (`postcard` + `BLAKE3`,
per-scroll content IDs, `format_version`), with `--text` and `--json` as
human/debug views. It deliberately left implementation — crate placement, serde
derives, the CLI split, the determinism tests — to a later pass. This is that
pass's decision record.

Two independent programs must agree on the manifest bytes:

- **`emetc`** (crate `emet`, `emet/crates/emet/`) is the *writer*. It owns
  `ir::Scroll` / `ir::Glyph` (`emet/crates/emet/src/ir.rs`) and only needs
  `Serialize`.
- **`golemd`** (crate `golemd`, `crates/golemd/`) is the *reader* (ADR 0014).
  It needs `Deserialize` and the same schema, format, and hash.

ADR 0012 named this "a cross-repo contract" — but since Emet was embedded into
the golem monorepo (commit `f918b75`), it is now a **cross-crate contract inside
one Cargo workspace**. That materially changes the safest implementation: the
schema can be a single shared crate that both writer and reader depend on,
rather than a duplicated-and-version-checked struct in two repos. The manifest
still carries `format_version` for on-the-wire safety, but the two ends can no
longer *drift* — they compile against the same types.

The workspace already provides `serde`, `blake3`, `sha2`, `hex`, and
`ed25519-dalek` (`Cargo.toml`). It does **not** provide `postcard`; ADR 0012
selected postcard as the serialization format, so this ADR must add it.

The dependency-direction constraint is the crux. `emet` today has a deliberately
tiny footprint (`ariadne`, `chumsky` only — `emet/CLAUDE.md`). `golemd` is a
heavier binary (axum, tokio, rusqlite). Neither should depend on the other, and
`emet` must not absorb `golemd`'s weight. So the shared schema cannot live in
either `emet` or `golemd`.

## Decision

### 1. A new shared crate: `scroll-format`

Create a new workspace member, **`scroll-format`**, at `crates/scroll-format/`,
that owns the wire contract and nothing else:

- The **glyph/scroll data model**: `Glyph`, `Scroll` (moved here — see §2),
  `AddressedScroll`, `Manifest`, and `ContentId`.
- The **content-addressing function**: `content_id(&Scroll) -> ContentId` =
  `blake3(postcard::to_stdvec(scroll))`, and manifest assembly
  `Manifest::from_scrolls(scrolls, emet_version) -> Manifest`.
- **Serialization helpers**: `to_bytes(&Manifest) -> Vec<u8>` /
  `from_bytes(&[u8]) -> Result<Manifest>` over postcard; and a `--json` view via
  `serde_json` behind a feature/free function (the JSON view is not
  content-addressed — ADR 0012 §2).
- The `format_version` constant and a `check_format_version` on read.

Dependencies of `scroll-format`: `serde` (+derive), `postcard`, `blake3`, `hex`,
and `serde_json` (for the debug view only). All already in the workspace except
`postcard`, which is **added to `[workspace.dependencies]`**.

`scroll-format` names no I/O, no filesystem, no network — it is pure data +
pure functions. Both `emet` and `golemd` depend on it; it depends on neither.
Dependencies point *toward* this stable, tiny crate — the ports-and-adapters
"stable centre" rule (`lw:architect`, `lw:hexagonal`). Its directory name says
what it is (the scroll wire format), not how it is built (`lw:screaming`).

### 2. The glyph/scroll model moves to `scroll-format`; `emet::ir` re-exports

`ir::Scroll` / `ir::Glyph` are today defined in `emet/crates/emet/src/ir.rs`.
They **move to `scroll-format`** and `emet::ir` **re-exports them**
(`pub use scroll_format::{Glyph, Scroll};`) so the rest of the compiler
(`eval.rs`, `lib.rs`, `main.rs`, `infer.rs`'s glyph types) is untouched in spirit
and the language keeps treating these as "the IR." This is the model-
reconciliation move; ADR 0014 covers why golem's *old* rich model
(`golem-types`) is retired rather than reconciled field-by-field.

Rationale for moving rather than duplicating: a single definition cannot drift,
and ADR 0012's whole correctness argument rests on the reader and writer hashing
*the same bytes from the same struct layout*. Duplication would reintroduce the
cross-repo drift hazard the monorepo just removed.

`emet::ir` keeps its `key()` / `describe()` inherent methods where they are
compiler/plan-rendering concerns, OR moves them alongside the types — the
implementer decides, but the **serialized fields** (name, and each glyph's
fields) are what `format_version` pins; helper methods are not part of the wire
contract.

### 3. Serde derives and field-order discipline

`Glyph` and `Scroll` (now in `scroll-format`) gain
`#[derive(Serialize, Deserialize)]` alongside their existing
`Debug, Clone, PartialEq, Eq`. `Manifest`, `AddressedScroll`, and `ContentId`
derive the same.

Because postcard is **non-self-describing** (ADR 0012 §3), struct field order and
enum variant order **are** the encoding. A `// NOTE:` linking ADR 0012/0013 sits
on each type; reordering a field or variant, or adding one, is a
`format_version`-bumping change, not a free refactor. `ContentId` is a newtype
over `[u8; 32]` with a lowercase-hex `Display`/`FromStr` for the string form.

### 4. CLI mode split in `emetc` (`emet/crates/emet/src/main.rs`)

Per ADR 0012 §2, `main.rs` moves from "always print text" to a mode split:

- **default** — write the binary manifest: to `-o PATH` if given, else raw bytes
  to stdout with **no log chatter** on stdout (diagnostics go to stderr), so
  `emetc build FILE | golemd load -` composes.
- **`--text` / `--human`** — today's `describe()` plan, now opt-in; the debug/
  eyeball path, explicitly not the contract.
- **`--json`** — the self-describing JSON view of the same manifest, for humans/
  ad-hoc tooling; **not** content-addressed, never the artifact golemd consumes.

`emetc build FILE` is the natural subcommand name; the exact clap shape is an
implementation detail left to Phase 1.

### 5. Determinism as a tested invariant

The content ID's correctness *is* serialization determinism (ADR 0012 §3–4). The
implementation pass must add, in `scroll-format`'s tests, at minimum:

- **Stability**: a fixed `Scroll` value serializes to a byte-for-byte constant
  (golden bytes committed) and hashes to a constant `ContentId`.
- **Round-trip**: `from_bytes(to_bytes(m)) == m` for a representative `Manifest`.
- **Independence**: a scroll's `content_id` is invariant to its position in the
  manifest and to `emet_version` (ADR 0012 §1).
- **Format-version guard**: `from_bytes` on a manifest with an unknown
  `format_version` is a clean, typed error, not a panic or a misparse.

These rank with `emet`'s `tests/layout.rs` / `tests/pipeline.rs` as spec-level
tests (`emet/CLAUDE.md`).

## Alternatives considered

1. **Put the schema in `golem-types` and have `emet` depend on it.** Rejected:
   `golem-types` is the *old* rich model (Blueprint/Host/Workload/Service/
   Ingress) that ADR 0014 retires, and making `emet` depend on golem's core
   would invert the footprint (the tiny language crate pulling golem's model).
   The clean move is a *new* minimal crate; `golem-types` is gutted separately.
2. **Define the schema in `emet` and have `golemd` depend on `emet`.** Rejected:
   `golemd` would then transitively pull `chumsky`/`ariadne` and the whole
   compiler to read a manifest — the reader depending on the writer's internals.
   The port (the wire types) belongs to neither side; it is its own stable crate.
3. **Duplicate the structs in `emet` and `golemd`, version-checked.** This is
   ADR 0012's cross-*repo* fallback. Rejected now: we are in one workspace, so a
   shared crate removes drift entirely. `format_version` is retained anyway for
   on-disk/older-artifact safety, but it should never be the *only* guard when a
   shared type is available.
4. **Skip `postcard`; reuse a crate already in the workspace (`serde_json`).**
   Rejected: ADR 0012 pinned postcard for determinism-by-construction and a
   compact binary. JSON is the debug view, not the contract. Adding one
   single-purpose crate is the accepted, bounded cost.

## Consequences

- **New workspace member `crates/scroll-format/`** and **`postcard` added to
  `[workspace.dependencies]`**. `emet` and `golemd` both gain a `scroll-format`
  dependency; `emet` gains `serde`/`postcard`/`blake3` transitively (its
  footprint grows, as ADR 0012 already accepted).
- **`ir::Scroll`/`ir::Glyph` move out of `emet`** and are re-exported. Any code
  or docs pointing at "`ir.rs` is the sole output" is now "`ir` re-exports the
  shared `scroll-format` model"; `emet/CLAUDE.md`'s IR section needs a doc pass
  (owned by the documenter, `PLAN.md`).
- **The manifest schema has exactly one definition**, eliminating the cross-repo
  drift hazard ADR 0012 flagged as its standing risk — while keeping
  `format_version` for artifact-at-rest compatibility.
- **`emetc`'s default output changes from text to binary** — a behaviour change
  for anyone (scripts, the docs) that piped `emetc`'s stdout expecting text; they
  move to `--text`.
- **Determinism becomes CI-enforced** via golden-byte and round-trip tests.
- **Cross-references:** implements ADR 0012 (binary content-addressed output);
  the shared model it defines is consumed by ADR 0014 (golemd rewrite) and drives
  the versioning in ADR 0015 (reversible reconcilers). Builds on ADR 0009
  (`Scroll` container) and ADR 0004 (inert concrete IR).
