# PLAN — Emet-native golem: binary manifest, glyph golemd, Emet authoring

Master phased plan for the refactor that rips out Nickel and the old
JSON-instruction path and makes golemd consume the **binary content-addressed
output of the emetc compiler**, acting only on the four glyphs Emet produces.

This is a planning document. It orders the work, states exit criteria and
dependencies per phase, names the agent/skill that executes each phase, and lists
the decisions that need the user's sign-off (**[RATIFY]**) before or during
execution.

Design records this plan realizes:

- **ADR 0012** (`docs/adr/`) — binary content-addressed compiler output
  (the accepted plan).
- **ADR 0013** — implement the binary manifest; the shared `scroll-format` crate.
- **ADR 0014** — golemd glyph rewrite + model reconciliation.
- **ADR 0015** — reversible reconcilers + content-addressed versioning.
- **ADR 0016** — Emet module system (Elm-modeled).

## Execution conventions

- **Implementation** is done by the **`/lw:implementer`** agent, writing
  **strictly ZERO comments** — naming and structure are the only documentation.
- **All comments, doc-comments, READMEs, and ADR/doc prose** are owned by a
  separate **`/lw:documenter`** agent, run after the implementer on each phase.
- **User-facing communication / sign-off** goes through **`/lw:communicator`**.
- Tests are written test-first (`superpowers:test-driven-development`) where the
  behaviour is specifiable up front — the determinism tests (Phase 1) and the
  `reverse(apply(x))` isometry tests (Phase 3) especially.
- Per `apps/emet/CLAUDE.md`: run `cargo test -p emet -p emet-lsp` (and the golem
  crates) after each change; small commits per fix. **Git is user-driven only**
  (root `CLAUDE.md`) — agents do not commit unless asked.

## ADR placement decision

**[RATIFY 0]** These four ADRs (0013–0016) are written into **`docs/adr/`**,
continuing the emet sequence that 0012 started. Rationale: the wire contract and
the model both originate at the emet boundary, 0012 lives there, and the sequence
is unbroken. **Alternative:** a new golem-side ADR series for golem-ecosystem
decisions, keeping emet's ADRs language-only. Recommendation: keep them in
`docs/adr/` now; if the golem side later accrues its own ADRs unrelated to
emet, start a separate series then.

---

## Phase 0 — Model-reconciliation gate (the true first gate)

**This gate blocks everything.** It is a *decision-ratification* phase, not a
large code phase: confirm the shared-model shape so Phases 1–5 build on stable
ground.

- Ratify the shared-model decisions: `scroll-format` as the single shared crate
  (ADR 0013 §1); `ir::Scroll`/`ir::Glyph` **move** there and `emet::ir`
  re-exports (0013 §2); golem's rich `golem-types` model is **deleted** (0014
  §1); `golem-types` is **removed as a member** vs repurposed for golemd-only
  journal types (0014 §1 recommendation: remove).
- Ratify the reconciler contract shape (ADR 0015 §1) and the CID-driven
  diff (0015 §2) at the design level, since Phases 2–3 depend on it.

**Executes:** `/lw:communicator` relays the [RATIFY] list below to the user;
`/lw:architect` thinking already baked into the ADRs.
**Depends on:** the four ADRs (written).
**Exit criteria:** every **[RATIFY]** item below has a user decision recorded.
**Risk:** ratifying the wrong shared-crate boundary forces rework of Phases 1–2;
mitigated by the ADRs' explicit alternatives.

---

## Phase 1 — The binary manifest + `scroll-format` crate (realizes ADR 0012/0013)

Stand up the shared wire contract so both ends compile against one model.

- Create `libs/scroll-format/` (new workspace member). Move `Glyph`/`Scroll`
  in; add `AddressedScroll`, `Manifest`, `ContentId`; `content_id` = BLAKE3 over
  postcard bytes; `Manifest::from_scrolls`; `to_bytes`/`from_bytes`; the JSON
  debug view; `format_version` + guard (ADR 0013 §1–3).
- Add `postcard` to `[workspace.dependencies]`.
- `emet::ir` re-exports the moved types (0013 §2). Compiler otherwise unchanged.
- `emetc` CLI mode split: binary default (`-o`/stdout, clean stream), `--text`,
  `--json` (0013 §4).
- **Determinism tests** (test-first): golden bytes, round-trip, position/
  `emet_version` independence, format-version guard (0013 §5).

**Executes:** `/lw:implementer` (code, zero comments) → `/lw:documenter`
(module docs, `// NOTE:` links to ADR 0012/0013 on wire-order-sensitive types).
**Depends on:** Phase 0.
**Exit criteria:** `scroll-format` builds and its determinism tests pass;
`emetc build FILE` emits a binary manifest by default and `--text`/`--json`
reproduce today's views; `emet`/`emet-lsp` tests still green.
**Risk:** low. postcard field-order fragility is contained by the golden-byte
test. This phase is largely independent of golemd and can start immediately after
Phase 0.

---

## Phase 2 — golemd glyph rewrite + model reconciliation (ADR 0014)

Swap golemd's model and input; keep the good bones (retry spine, per-host filter,
PlanRoom shape).

- Delete the rich model from `golem-types` (and remove/repurpose the crate per
  the Phase-0 ratification).
- golemd depends on `scroll-format`. New ingest port: manifest bytes → `Manifest`
  → select this host's scroll (0014 §2).
- Reshape into a pure diff core + `Reconciler` port (0014 §3) — the port itself
  and its adapters are Phase 3; this phase defines the port and wires the core +
  the **fake in-memory reconciler** so the diff/enact/journal spine is testable
  with zero host I/O.
- Rework `PlanRoom` storage to AddressedScroll + ordered glyph ops (0014 §4);
  collapse `RevisionKind` to `Init`/`Reconcile`.
- `golemctl`: delete the `nickel` shell-out; `apply <file.emet|manifest.bin>`
  drives `emetc` or ships bytes (0014 §5). HTTP: `POST /manifest`, adjusted
  `/state`, surviving `/revisions`/`/status`.

**Executes:** `/lw:implementer` → `/lw:documenter`.
**Depends on:** Phase 1 (`scroll-format`), Phase 0 ratifications (0014 §1/§4/§5).
**Exit criteria:** golemd ingests a real emetc manifest, diffs against stored
state via the **fake reconciler**, and journals glyph ops; the rich-model code
(Workload/Service/Ingress/quadlet/ingress) is gone; golemctl + HTTP updated;
golemd tests green against the fake.
**Risk:** medium — largest deletion. Mitigated by keeping the retry spine and
PlanRoom shape and testing against the fake reconciler before any real adapter.

---

## Phase 3 — Reversible reconcilers + content-addressed versioning (ADR 0015)

Make the four glyphs real and isometric.

- Implement the `Reconciler` port's `apply`/`reverse` returning `Outcome`/
  `Inverse` (0015 §1); the CID-driven Install/Remove/Replace/Noop diff (0015 §2,
  now against real adapters); journal the ordered `Outcome` list as the reversal
  record (0015 §3).
- Implement the four concrete reconcilers — apt / systemd / file / lineInFile —
  each capturing prior state for exact reversal (0015 §4).
- Idempotency, LIFO ordering/rollback, all-or-nothing (0015 §5).
- **Property tests**: `reverse(apply(x))` returns the host to its pre-apply state,
  per glyph (via the fake host + later a real Debian box).

**Executes:** `/lw:implementer` (test-first for the isometry laws) →
`/lw:documenter`.
**Depends on:** Phase 2 (the port + core + journal exist).
**Exit criteria:** each glyph reconciler passes its isometry property test; an
end-to-end reconcile → upgrade (CID change) → decommission (empty scroll) cycle
against a real Debian box installs, upgrades, and fully reverses.
**Risk:** medium-high — real host effects. Mitigated by the fake-adapter spine
from Phase 2 and per-glyph property tests before touching a box. **[RATIFY]**
items: large-`file` inverse storage; rollback-vs-resume on partial failure.

---

## Phase 4 — Re-author the lichess examples in Emet (ADR 0016)

Prove the authoring surface; build only the abstractions lichess needs.

- **4a (de-risking spike, can start after Phase 1):** port one lichess host
  (e.g. `scaly` — simplest, one networkless workload) to a **single Emet file**,
  no module system, to validate the glyph-lowering of the abstractions and the
  cross-host-reference-as-value approach (0016 §4).
- **4b:** build the **minimal Elm-modeled module system** (0016 §2): `module …
  exposing`, `import … [as …] [exposing …]`, file=module, qualified access via
  the ADR 0006 resolver, single-`main` preserved. New pre-inference
  name-resolution/import-graph stage.
- **4c:** author a shared `Lichess`/`Fleet` library of abstractions (workload,
  service, ingress-as-firewall+glyphs, cross-host refs as values) and re-author
  all lichess hosts against it; each compiles to a `Scroll` of the four glyphs.
- Update `examples/lichess/run.sh` to compile `.emet` → manifest → `golemctl
  apply`, dropping `nickel export`.

**Executes:** `/lw:implementer` (language + examples) → `/lw:documenter`.
**Depends on:** Phase 1 (manifest, for 4a end-to-end); 4c depends on 4b; the
end-to-end run depends on Phases 2–3 for a golemd that enacts.
**Exit criteria:** every lichess host has an Emet source that compiles to the
expected scroll; `run.sh` drives the full emetc→manifest→golemd path.
**Risk:** the module system is new language work. **Mitigated by 4a** proving the
abstractions first, and by the ADR 0016 finding that **list patterns are NOT
required** for this port (they stay an independent language-backlog item). If the
value-level cross-host-ref approach proves awkward, the `ref`-helper fallback
(0016 §4) unblocks without a language change.

---

## Phase 5 — Docs cutover: new model in, old terminology out (goal #4)

- Update the Astro/Starlight site (`sites/website/src/content/docs/`) for the glyph/
  scroll/manifest model: rewrite `reference/primitives`, `reference/architecture`,
  `reference/bundle-format` (→ manifest format), getting-started, and guides to
  the Emet authoring surface + the four glyphs.
- **Drop the old terminology docs**: `TERMINOLOGY.md`, `TERMINOLOGY.discworld.md`,
  and the Blueprint/Workload/Service/Ingress vocabulary in the root `CLAUDE.md`
  and docs — replaced by glyph/scroll/fleet (and the reconcile/reverse/CID model).
- Fold Emet's markdown docs into the site (`docs/TODO.md` §B "unify docs").

**Executes:** `/lw:documenter` (owns all prose) with `/lw:communicator` for any
user-facing wording calls.
**Depends on:** Phases 1–4 (the model must be real before the docs describe it as
current). The root `CLAUDE.md`'s "docs/ describes an older model — leave it
alone" caveat lifts here.
**Exit criteria:** the site builds and describes the Emet/glyph/manifest model
with no surviving Blueprint/Workload/Service/Ingress or Nickel references; old
terminology files removed.
**Risk:** low; last because it documents settled behaviour.

---

## Recommended ordering summary

**0 → 1 → 2 → 3**, with **Phase 4a** (single-file lichess spike) startable in
parallel right after Phase 1, **4b/4c** proceeding in parallel with Phases 2–3
(language work is independent of golemd), and everything converging before
**Phase 5** (docs). The model-reconciliation gate (0) unblocks the shared crate
(1), which unblocks both the golemd track (2→3) and the authoring track (4);
docs (5) come last because they describe the finished model.

## Consolidated risks

1. **Largest deletion is Phase 2** (rich model out of golemd). Mitigated by
   reusing the retry spine + PlanRoom and testing against a fake reconciler first.
2. **Real host effects in Phase 3.** Mitigated by per-glyph isometry property
   tests before any real box, and journalled `Inverse` bounding what golem undoes.
3. **New language work in Phase 4b (modules).** Mitigated by the 4a spike and by
   ADR 0016's finding that list patterns are not on this critical path.
4. **postcard field-order fragility (Phase 1).** Contained by golden-byte tests
   and `format_version`.
5. **Cross-host references** were Nickel's one genuinely fleet-global feature;
   re-expressing them as ordinary Emet values (0016 §4) is more principled but
   unproven — the 4a spike exists partly to validate it.

---

## [RATIFY] — decisions needing the user's sign-off

Each: the decision, the recommendation, the key alternative.

- **[RATIFY 0] ADR location.** Recommend keeping 0013–0016 in `docs/adr/`
  (continue the 0012 sequence). Alternative: a separate golem-side ADR series.
- **[RATIFY 1] Shared model = new `scroll-format` crate; `ir::Scroll`/`Glyph`
  move there, `emet::ir` re-exports.** Recommend as written (single definition,
  no drift). Alternative: schema in `golem-types` (rejected — that model is being
  deleted) or duplicated + version-checked (rejected — same-workspace).
- **[RATIFY 2] Delete golem's rich model; `golem-types` removed as a member**
  (golemd owns its journal types). Recommend remove. Alternative: repurpose
  `golem-types` for golemd-only journal types.
- **[RATIFY 3] `RevisionKind` collapses to `Init` / `Reconcile`** (decommission =
  reconcile toward empty scroll). Recommend as written. Alternative: keep an
  explicit `Decommission` kind.
- **[RATIFY 4] golemd's local journal may stay JSON** for legibility even though
  the wire format is binary postcard. Recommend allow JSON locally. Alternative:
  store postcard bytes in the journal too.
- **[RATIFY 5] HTTP surface**: `POST /manifest` replaces `POST /blueprints`;
  decommission-by-name removed (state is a whole scroll; "remove all" = apply
  empty scroll). Recommend as written. Alternative: keep a decommission verb.
- **[RATIFY 6] Large `file` `Inverse` storage**: inline prior bytes in the
  journal for the first cut. Recommend inline-first. Alternative: content-address
  the prior contents into a blob store from day one.
- **[RATIFY 7] Partial-failure behaviour**: LIFO-rollback the Outcomes applied
  this reconcile and journal nothing (matches the old all-or-nothing spine).
  Recommend rollback. Alternative: journal partial progress and resume.
- **[RATIFY 8] Module system now, minimal + Elm-shaped** (`module/exposing/
  import/as`, file=module, qualified access reusing ADR 0006); list patterns
  **not** required for the lichess port and sequenced separately. Recommend as
  written, with a single-file 4a spike first. Alternative: no module system
  (one-file lichess), or full Elm module system now.
- **[RATIFY 9] Cross-host references as ordinary Emet values** (shared fact table
  in an imported module), replacing Nickel's placeholder substitution. Recommend
  the value approach (no templating, ADR 0004). Alternative: a `ref`-string
  helper mirroring Nickel.
