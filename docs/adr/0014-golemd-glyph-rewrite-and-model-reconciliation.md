# 0014-golemd-glyph-rewrite-and-model-reconciliation

## Status

Accepted.

## Context

`golemd` (`crates/golemd/`) is built on golem's *old, rich* model
(`crates/golem-types/src/lib.rs`): `Blueprint → Host → { Workload, Service,
Ingress }`, resolved into a `State` (per-host, per-item-kind maps of item →
owning-blueprint names), diffed into `Action`s
(Build/Teardown ×Workload/Service/Ingress), and journalled as `Revision`s. The
`Foreman` (`foreman.rs`) holds blueprints, resolves state, drives a `Builder`
(`builder.rs` — a trait over the three item kinds) with retry, and journals via
a `PlanRoom` (`planroom.rs`, sqlite/memory). `golemctl` (`crates/golemctl/`)
shells out to `nickel export` to produce a `Blueprint` JSON and POSTs it.

The whole authoring surface is Nickel (`nickel/*.ncl`,
`examples/lichess/*.ncl`), which exports JSON matching `golem-types::Blueprint`.

The project direction (root `CLAUDE.md`; `emet/docs/TODO.md` §B) is: **Emet
replaces Nickel; the emetc binary manifest (ADR 0012/0013) replaces the
JSON-instruction path; and golemd is simplified to only the four glyphs Emet
actually produces** — `aptPackage`, `systemdService`, `file`, `lineInFile`
(`emet/crates/emet/src/ir.rs`, ADR 0002/0009). Workload/Service/Ingress,
quadlets, and ingress are removed from golemd entirely; they are to be rebuilt
*later, in the Emet language itself* as higher-level abstractions that compile
*down* to the four primitives — not as golemd resource kinds.

This forces the model-reconciliation question (`emet/docs/TODO.md` §B headline).
Two vocabularies describe desired state — golem's `Blueprint`/`State`/`Action`
and Emet's `Scroll`/`Glyph`. They must become one, and golem-types was named "the
source of truth." That framing predates the decision to make Emet the authoring
language. With Emet authoritative, the compiled `Scroll` *is* the desired state;
golem's rich `Blueprint` model is exactly what is being torn out.

## Decision

### 1. Retire golem's rich model; the Scroll/Glyph model is the domain

`Blueprint`, `Host`, `Workload`, `Service`, `Ingress`, `IngressFrom`, and the
old `State`/`HostState`/`Action`/`RevisionKind` **are deleted** from
`golem-types`. golemd's domain becomes the **`scroll-format` model** (ADR 0013):
one `Scroll` per host, each a list of `Glyph`s over the four kinds.

The "source of truth" inverts: the shared **`scroll-format`** crate (ADR 0013) is
now the model both the writer (emetc) and reader (golemd) share.
`golem-types` is either **emptied and removed** as a workspace member, or
**repurposed** to hold golemd-only domain types that are *not* part of the wire
contract — the applied-state journal types of §4. Recommendation: **remove
`golem-types`** and let golemd own its journal types in a `golemd` module,
keeping the wire model in `scroll-format`; this avoids a second "shared" crate
that only golemd uses. [RATIFY in PLAN.md]

### 2. golemd ingests the manifest, selects its host's scroll

golemd's new input is the **binary Manifest** (ADR 0013), not a JSON Blueprint.
The ingest port is small:

- Receive manifest bytes (over HTTP `POST /manifest`, or `golemd load -` from a
  pipe — the transport is an adapter detail).
- `scroll_format::from_bytes` → `Manifest`; check `format_version`.
- Select the `AddressedScroll` for **this golemd's `--host`** by matching
  `scroll.name` against the host identity (as the old foreman filtered actions by
  host). A manifest carries the whole fleet; a node enacts only its own scroll.
- Hand that `(content_id, scroll)` to the reconcile spine (§3).

golemd performs **no versioning inside emetc's world** — emetc supplies content
IDs, golemd *uses* them: the `content_id` of the selected scroll (and of each
glyph within it — see ADR 0015) is what drives upgrade/no-op/removal decisions.

### 3. The domain core: a pure reconcile planner behind a Reconciler port

Reshape golemd along ports-and-adapters (`lw:hexagonal`), keeping the parts of
the old design that were already right (the retry spine, the per-host filter, the
append-only journal) and dropping the item-kind sprawl:

- **Domain core (pure).** Given the *prior applied state* (what this node last
  enacted — ADR 0015's record) and the *desired scroll* (from the manifest),
  compute the ordered list of **glyph operations** needed: `Install(cid, glyph)`,
  `Remove(cid, glyph)`, `Replace(old_cid → new_cid, glyph)`, or `Noop`. This is a
  pure fold over two content-addressed glyph sets — the unidirectional
  desired-vs-actual diff (`lw:unidirectional-data-flow`): desired state in, an
  ordered plan out, no side effects. It replaces `State::actions_from` but works
  per-glyph on content IDs instead of per-item-name on blueprint sets.
- **`Reconciler` port** (replaces `Builder`). One narrow trait the core calls to
  *enact* a glyph operation and to *record* what it did for exact reversal. ADR
  0015 specifies its `apply`/`reverse` contract and the four concrete glyph
  reconcilers. The core names no apt, no systemd, no filesystem — only the port
  in glyph vocabulary (`lw:hexagonal` trap: the port is in the domain's words,
  not `AptAdapter`).
- **Adapters.** The real apt/systemd/file/lineInFile reconcilers (ADR 0015) are
  the driven adapters; a **fake in-memory reconciler** implements the same port
  for tests, so the whole diff+enact spine is exercised with zero host I/O — the
  way the old `Recorder`/`FlakyThenOk` fakes exercised the `Builder`.
- **The retry/attempt spine survives** (`foreman.rs::attempt`, retryable vs fatal,
  all-or-nothing persistence): it is orthogonal to the model and reusable as-is
  around the new `Reconciler`.

### 4. Journal and store in glyph terms; the PlanRoom shape survives

The `PlanRoom` port and its sqlite/memory adapters survive nearly intact — the
*append-only journal + local store* shape is model-agnostic and was well-factored.
What changes is *what* it stores:

- **Stored desired state** becomes the current `AddressedScroll` for this host
  (the last manifest the node accepted), replacing the map of `Blueprint`s.
- **The journal `Revision`** still is an append-only entry per change, but embeds:
  the `content_id` of the scroll enacted, the ordered glyph operations, and — the
  new, load-bearing part — the **reversal record** per glyph (ADR 0015) that lets
  a later change or a decommission exactly undo it. `RevisionKind` collapses to
  `Init` / `Reconcile` (a `Decommission` is just reconciling toward the empty
  scroll). [RATIFY the kind set in PLAN.md]
- Bodies are stored as **postcard/`scroll-format` bytes or JSON** in sqlite; the
  old code stored `serde_json` strings, which still works for a debugging-friendly
  journal. The *wire* contract is binary; the *local journal* format is golemd's
  private choice and may stay JSON for legibility. [RATIFY]

### 5. golemctl and HTTP follow the manifest

- `golemctl commission <bp.ncl>` (nickel) → `golemctl apply <file.emet | manifest.bin>`:
  it either runs `emetc` to compile a `.emet` source to a manifest, or ships a
  prebuilt `.bin`, then POSTs the bytes. The `nickel export` shell-out is deleted.
- HTTP: `POST /manifest` (was `POST /blueprints`), `GET /state` becomes "the
  current applied scroll + its content id," `GET /revisions[/:id]` and `/status`
  survive. Decommission-by-name goes away (a node's state is a whole scroll now,
  not a set of named blueprints); "remove everything" is applying an empty scroll.
  [RATIFY the exact HTTP surface in PLAN.md]

## Alternatives considered

1. **Reconcile the two models field-by-field (keep Blueprint, map glyphs onto
   Workload/Service/Ingress).** Rejected: the explicit goal is to *remove* the
   rich kinds from golemd and rebuild them in Emet. Mapping glyphs back onto
   Workload/Service would preserve exactly the model being retired and add a
   lossy translation layer.
2. **Keep `golem-types` as the source of truth and translate Scroll→Blueprint at
   the boundary.** Rejected for the same reason, plus it inverts authority: with
   Emet authoritative, the compiled Scroll *is* the truth; a Blueprint layer
   would be a second, redundant model to keep in sync.
3. **Keep Workload/Service/Ingress in golemd "for later."** Rejected: they are
   dead weight now and their eventual form is an *Emet-language* abstraction that
   compiles to the four primitives, not a golemd kind. Carrying them violates
   YAGNI and keeps the quadlet/ingress code paths alive with no producer.
4. **Rewrite golemd from scratch.** Rejected: the retry spine, per-host
   filtering, the PlanRoom port, and the fake-adapter test style are already
   correctly factored and model-agnostic. The rewrite is a *model swap plus a
   Builder→Reconciler swap*, reusing the good bones.

## Consequences

- **`golem-types`'s rich model is deleted**; `scroll-format` (ADR 0013) is the
  shared domain. Whether `golem-types` is removed as a member or repurposed for
  golemd-only journal types is [RATIFY]'d in `PLAN.md` (recommendation: remove).
- **`builder.rs` (`Builder`, `RandomBuilder`) is replaced by the `Reconciler`
  port + adapters** (ADR 0015). The Workload/Service/Ingress trait methods,
  `Named` impls, and item-kind `find` machinery in `foreman.rs` are deleted.
- **`foreman.rs` shrinks to: ingest manifest → select host scroll → pure diff →
  enact via `Reconciler` (with the surviving retry spine) → journal.** It is no
  longer "the foreman over three item kinds" but "the reconcile loop over glyphs."
- **`planroom.rs` survives structurally**; its stored types change from
  Blueprint/State/Action to AddressedScroll + glyph ops + reversal records.
- **`golemctl` loses the `nickel` shell-out**; it drives `emetc` (or ships a
  prebuilt manifest). The Nickel authoring surface (`nickel/`, the `.ncl`
  examples) is retired in `PLAN.md`'s cutover phase.
- **A large amount of code is deleted**, which is the point: golemd becomes "apply
  the four primitives from a content-addressed scroll," nothing more.
- **Cross-references:** consumes the shared model + manifest of ADR 0013;
  delegates the enact/reverse contract and the four concrete reconcilers to ADR
  0015; the higher-level abstractions removed here reappear as Emet library code
  authored per ADR 0016. Supersedes the rich-model framing in the root
  `CLAUDE.md` and `TERMINOLOGY.md`.
