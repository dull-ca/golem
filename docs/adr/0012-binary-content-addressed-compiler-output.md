# 0012-binary-content-addressed-compiler-output

## Status

Proposed.

> **This ADR is a plan. It records the design and the recommendation; it does
> not implement anything.** No `serde` derives, no format crate, no hashing code
> land as part of accepting it. Implementation is a separate pass.

## Context

`emetc` today evaluates a program to `Compiled { scrolls: Vec<Scroll> }`
(`src/lib.rs`) and `main.rs` prints it as human-readable text — `main : List
Scroll` followed by an indented, `describe()`-rendered plan. That output is for
a person reading a terminal; nothing consumes it programmatically.

In the larger **golem** ecosystem this is the wrong bottom. A daemon,
**`golemd`**, runs on each host, takes the latest *versioned* scroll for that
host, and enforces it: reconciliation of the glyph primitives against the real
machine, content-addressable versioning, and (eventually) cryptographic
signing. `golemd` owns an RPC over which it receives scrolls.

`emetc` **must stay ignorant of all of that.** Its job begins and ends at "given
source, produce the compiled fleet." But the fleet has to leave `emetc` in a
form a machine — specifically `golemd` — can consume. Concretely that form
must:

- be a **binary artifact**, not human text (text is not the machine interface);
- carry a **content ID per scroll** — a hash of that scroll — so `golemd` can do
  content-addressable versioning *without `emetc` doing any versioning itself*;
- be **deterministic**: the content ID is a hash of the serialized scroll, so
  two runs that produce an identical `Scroll` MUST produce identical bytes and
  therefore an identical content ID. Non-canonical serialization silently breaks
  content addressing.

There is a hard boundary running through this. `emetc` produces content-addressed
scroll bytes and stops. `golemd` owns transport, versioning, diffing,
reconciliation, and signing. The artifact `emetc` emits is therefore a **wire
contract shared between two repos** — a cross-repo agreement, not an internal
detail — which raises coordination and versioning concerns this ADR must name.

The current `ir` types (`src/ir.rs`) derive only `Debug, Clone, PartialEq, Eq`.
Serialization needs `serde::Serialize` on `Glyph` and `Scroll`. The project also
holds a **small-footprint** value (`CLAUDE.md`: `ariadne` for diagnostics,
`chumsky` only if the parser migrates); any new dependency is spent against that
value and must be justified.

## Decision

**Change `emetc`'s primary output from human text to a deterministic, binary,
content-addressed serialization of the compiled fleet. Keep the text output
behind a flag for humans.** Concretely:

### 1. New output: a versioned manifest of content-addressed scrolls

`emetc` emits a serialized top-level **manifest**:

```
Manifest {
    format_version: u32,        // the wire-contract version; bumped on schema change
    emet_version:   String,     // compiler version that produced this (provenance; not hashed)
    scrolls: Vec<AddressedScroll>,
}

AddressedScroll {
    content_id: ContentId,      // hash(canonical_serialization(scroll))
    scroll:     Scroll,         // the ir::Scroll, verbatim
}
```

- **`content_id` is over the `scroll` alone**, never over the `AddressedScroll`
  wrapper and never over the `Manifest` — otherwise the ID of a scroll would
  depend on its neighbours or on `emet_version`, defeating content addressing.
- **`format_version` is the contract version**, checked by `golemd` on receipt.
  It is distinct from `emet_version` (compiler provenance / debugging). A schema
  change to `Scroll`, `Glyph`, the manifest shape, the serialization format, or
  the hash algorithm bumps `format_version`.
- **`emet_version` is provenance only** and is deliberately *outside* every
  hash, so rebuilding an unchanged fleet with a newer compiler yields the same
  per-scroll content IDs.
- `Compiled` (`src/lib.rs`) already carries `Vec<Scroll>`; the manifest is
  assembled from it. Computing content IDs is a small, pure function of the
  scrolls — it fits inside the compiler with no new concepts leaking in.

### 2. CLI shape

- **Default: emit the binary manifest.** Write to a path when given
  (`emetc build FILE -o fleet.bin`); otherwise write the raw bytes to stdout, so
  `emetc build FILE | golemd load` composes. stdout in binary mode emits *only*
  the artifact — no log chatter — so the stream stays clean for piping.
- **`--text` / `--human`: today's readable plan** (`main : List Scroll` + the
  indented `describe()` output). Unchanged behaviour, now opt-in. This stays the
  debugging and eyeball path and is explicitly **not** the machine contract.
- **`--json` (debug only):** a self-describing JSON rendering of the same
  manifest, for humans and ad-hoc tooling. It is a *view*, never the artifact
  `golemd` consumes, and its bytes are **not** content-addressed (JSON is not
  our canonical form — see §3).

### 3. Serialization format — **postcard**

The content ID is `hash(serialize(scroll))`, so the single most important
property is **canonical / deterministic serialization**: identical `Scroll`
values must serialize to identical bytes on every run, platform, and compiler
version. Evaluated against that criterion:

- **postcard (recommended).** Rust-native (`serde`), compact, no-std-friendly,
  and **deterministic by construction**: it is a non-self-describing format with
  a fixed encoding driven by the type's field order, so there are no map-key
  ordering or field-tagging degrees of freedom to canonicalize away. `Scroll`
  has a fixed field layout and `Glyph` is an enum with a stable variant order,
  which is exactly postcard's sweet spot. Same `Scroll` ⇒ same bytes, for free.
- **msgpack (`rmp-serde`).** Cross-language, compact, self-describing. But
  structs-as-maps means **map-key ordering is a canonicalization burden** we
  would have to enforce and defend to keep hashes stable; the determinism we
  need is not the default.
- **CBOR (`ciborium`).** Cross-language, self-describing, and it *has* a defined
  **canonical mode** (RFC 8949 §4.2). Strongest cross-language story with real
  determinism — but only if canonical mode is correctly and permanently engaged;
  it is a stricter, heavier encoding than we need for a Rust↔Rust link today.
- **bincode.** Rust-native and simple, but **not self-describing and
  version-fragile** — encoding tied to layout with weak evolution guarantees.
  Poor fit for a cross-repo *contract* that must evolve. Rejected.

**Recommendation: postcard**, because the `emetc`→`golemd` link is Rust↔Rust
today and postcard gives determinism *for free* (no canonicalization pass to get
right) plus a compact, minimal-dependency encoding. Determinism-by-construction
beats determinism-by-discipline when a hash depends on it.

**Cross-language fallback is pre-decided:** if `golemd` (or another consumer)
ever needs a non-Rust reader, migrate to **CBOR in canonical mode**, not
msgpack — CBOR's canonical form is a spec, not a convention we maintain. That
migration is a `format_version` bump, which the manifest already anticipates.

This decision **requires `#[derive(serde::Serialize)]` on `ir::Scroll` and
`ir::Glyph`** (`src/ir.rs`). `Deserialize` on the `emetc` side is optional (the
compiler only writes); `golemd` owns the read side in its own repo.

### 4. Content addressing — **BLAKE3**

`content_id = blake3(canonical_serialization(scroll))`, stored as its 32-byte
digest (rendered as lowercase hex where a string form is needed).

- **BLAKE3 (recommended):** fast, modern, Rust-native, single well-maintained
  crate; ample collision resistance for content addressing. Aligns with the
  "modern Rust toolchain" posture of the rest of the pipeline.
- **SHA-256 (alternative):** chosen only if ubiquity / interop with existing
  content-addressable stores or signing tooling demands the more universally
  implemented primitive. It buys interop at some speed cost.

**Recommendation: BLAKE3**, revisited only if `golemd`'s signing/versioning
layer standardizes on SHA-256 — a cross-repo call (see §5), not a compiler call.

**The hash is only as stable as the serialization under it.** The content ID's
correctness rests entirely on §3's canonical bytes; the hash choice and the
format choice are one coupled decision, and both are pinned by `format_version`.

The compiler **computes** content IDs and nothing more. It does **no**
versioning, diffing, dedup, or storage — those read the content IDs downstream.

### 5. Scope boundary — what `emetc` deliberately does NOT do

`emetc` produces the content-addressed manifest bytes and stops. Explicitly
designed **out** of the compiler and **into `golemd`**:

- **RPC transport** (the grpc-ish channel that ships scrolls to hosts). `emetc`
  emits bytes to a file or stdout; it opens no sockets and speaks no protocol.
- **Content-addressable *versioning*** — "latest versioned scroll for this
  host," history, diffing, dedup. `emetc` supplies the content IDs that make
  this possible; it does not track versions.
- **Reconciliation** of the isometric / reversible glyph primitives against a
  live machine.
- **Signing.** Cryptographic signatures over scrolls/manifests are `golemd`'s,
  layered on top of the content IDs. `emetc` never holds a key.

The manifest is therefore a **versioned wire contract between `emetc` and
`golemd`**, living in two repos. Its schema (`Manifest` / `AddressedScroll` /
`Scroll` / `Glyph`), its serialization format, and its hash algorithm must be
**agreed across both repos** and evolved in lockstep via `format_version`. This
is a real dependency and risk: `emetc` cannot change the artifact unilaterally
without breaking `golemd`, and vice versa. Flagged as the primary coordination
hazard of this decision.

## Alternatives considered

1. **Keep text-only output.** Rejected: not machine-consumable and carries no
   content IDs. This is the status quo the whole ADR exists to replace; it
   survives only as the `--text` debug view.
2. **JSON as the primary artifact.** Rejected as the contract: JSON is retained
   as a `--json` *debug view* but is not content-addressed and is not the wire
   format. Canonical JSON (sorted keys, number/whitespace normalization) is
   achievable but is determinism-by-discipline over a format designed for
   humans, when a binary format gives us determinism-by-construction and a
   smaller artifact.
3. **protobuf / a dedicated schema language (IDL) for the artifact.** Rejected
   for the *scroll blob*: heavier toolchain (codegen, `.proto` files) for a
   Rust↔Rust link, and protobuf's wire format is not canonical by default
   either. Note the boundary: `golemd`'s **RPC layer** may legitimately use
   protobuf/grpc for transport — but it can carry the scroll blob as
   format-agnostic opaque bytes, so the RPC's schema language and the blob's
   serialization are independent choices. This ADR fixes only the blob.
4. **msgpack for cross-language robustness now.** Rejected today: pays the
   canonical-map-ordering tax before any non-Rust consumer exists. If that
   consumer arrives, §3 pre-commits to **CBOR canonical mode** instead, as a
   `format_version` bump.
5. **Hash the whole manifest instead of per-scroll.** Rejected: `golemd` needs
   per-scroll (per-host) content IDs for per-host versioning; a single
   manifest-level hash would couple every host's identity to every other host's
   and to `emet_version`.

## Consequences

- **`src/ir.rs` gains `#[derive(serde::Serialize)]`** on `Scroll` and `Glyph`,
  and the `ir` types acquire a stable, externally-observed encoding — their
  field/variant order is now part of a wire contract, so reordering a `Glyph`
  variant or a struct field is a `format_version`-bumping change, not a free
  refactor. Code touching the serialization or hashing should carry a
  `// NOTE:` linking this ADR.
- **New dependencies beyond `ariadne`/`chumsky`:** `serde` (+ derive), the
  format crate (**postcard**), and the hash crate (**blake3**). This grows the
  footprint against the small-dependency value. Justified: producing a
  deterministic, content-addressed artifact is core to `emetc`'s purpose in the
  golem ecosystem, and each crate is single-purpose, mature, and Rust-native.
  This is a deliberate, bounded spend, not scope creep.
- **The CLI grows a mode split:** binary by default (stdout/`-o`), `--text` /
  `--human` for today's plan, `--json` for a debug view. `main.rs`'s current
  rendering moves behind `--text`.
- **A cross-repo contract now exists.** The artifact schema + format + hash are
  shared with the golem/`golemd` repo and must be coordinated and versioned
  together via `format_version`. This is the standing risk introduced here.
- **Determinism becomes a testable invariant:** "same `Scroll` ⇒ same bytes ⇒
  same content ID" is a property the implementation pass must assert (a
  round-trip / stability test), on par with how `tests/layout.rs` and
  `tests/pipeline.rs` pin the subtle subsystems.
- **Cross-references:** builds directly on the `Scroll` container (ADR 0009) and
  the inert-concrete-IR principle (ADR 0004) — content addressing is only
  meaningful because the IR is fully-evaluated concrete data with no templates
  or nondeterminism in it. Adding a glyph primitive (ADR 0002) now also means
  extending the serialized contract.