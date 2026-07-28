# 0030-apt-package-version-and-index-policy

## Status

Proposed 2026-07-24. Enriches the `aptPackage` glyph of ADR 0002, reconciled in
ADR 0015; a `Glyph` field change pinned by `format_version` per ADR 0012/0013;
mirrors the typed-attribute-sum modeling of ADR 0019. Sibling ADR 0029 is being
authored concurrently; this claims number 0030.

## Context

golemd owns exactly four glyphs (root `CLAUDE.md`): `aptPackage`,
`systemdService`, the filesystem glyph, and `lineInFile`. The standing invariant
is "there is no fifth resource kind" — richer shapes are Emet library
abstractions that *compile down* to these four, never new golemd reconcilers
(ADR 0002, ADR 0015 §Consequences). This ADR enriches the *existing*
`aptPackage` glyph; it does not add a kind.

Today `aptPackage` carries a single field and the reconciler is presence-only:

- **The wire model is a one-field record.** `Glyph::AptPackage { name: String }`
  (`libs/scroll-format/src/scroll.rs:24`). Its key is `apt:<name>`
  (`scroll.rs:110`). Every glyph field is a plain `String` except the filesystem
  `Perms` (ADR 0004/0019).
- **The reconciler checks presence only, never version.** `apply_apt`
  (`apps/golemd/src/reconcilers.rs:70`) returns a no-op the moment
  `apt_installed` is true — `dpkg-query -W -f=${Status}` containing
  `install ok installed` (`reconcilers.rs:85`). Any installed version satisfies
  the glyph; **once installed, golem never upgrades or pins.** There is no way to
  request "exactly 1.2.3" and no drift when the host holds a different version.
- **`apt-get update` runs per glyph.** Every `apply_apt` that installs pays a
  full index refresh first (`reconcilers.rs:74`), and the doc comment
  (`reconcilers.rs:66`) flags this as wasteful: "A single refresh per reconcile
  would be cheaper, but this stateless per-glyph adapter has no reconcile-scoped
  hook to hang one update on." A ten-package scroll runs `apt-get update` up to
  ten times.
- **Both apt failures are `Retryable`** (`reconcilers.rs:76`, `:80`).

Two capability gaps follow, and they are coupled:

1. **No version control.** The content id (BLAKE3 over the glyph's postcard
   bytes, ADR 0012) currently hashes only `name`, so two scrolls that want
   different versions of the same package are indistinguishable to the diff.
   golem cannot express a pin, cannot detect version drift, and has no
   golem-native "upgrade" — the deterministic upgrade primitive golem already
   owns for every other glyph (change source → content id changes → `Replace`)
   is unavailable because the version is not in the glyph.

2. **No index-freshness control, and it is the real driver.** The registry-style
   dogfood pattern is: an author adds a new apt source — a `file` glyph under
   `/etc/apt/sources.list.d/` — and then installs a package *from that source* in
   the same scroll. That install needs a fresh index, or `apt-get install` cannot
   resolve the package. Today the per-glyph `apt-get update` accidentally covers
   this (every install refreshes) but at the cost of refreshing on every install
   forever, including the common case where nothing about the sources changed.
   The author has no way to say "refresh because I just added a source" versus
   "don't waste a refresh, the index is fine."

Constraints that bound any answer (the same ones ADR 0019 weighed for the
filesystem glyph):

- **The wire format is non-self-describing** (ADR 0012/0013): a `Glyph`
  variant's field and variant order *is* the postcard encoding
  (`scroll.rs:17`). Adding fields to `AptPackage` — or new policy sums — is a
  `format_version` bump (`FORMAT_VERSION`, currently `2`, `manifest.rs:20`), not
  a free refactor. `check_format_version` cleanly rejects an old manifest rather
  than misparsing it.
- **The content id must stay the version-drift signal.** Whatever encodes the
  version must be *inside* the hashed glyph, so that bumping a pin produces a new
  content id and the pure diff (`reconcile::plan`) yields a `Replace` — golem's
  existing, deterministic upgrade path (ADR 0015 §2).
- **Reversibility is a property of the (glyph, prior-host-state) pair** (ADR
  0015): any new apply behavior needs an `Inverse` that restores exactly what
  apply changed, and golem must never remove or downgrade a package it did not
  install or change.
- **The reconciler port is per-glyph and stateless.** `Reconciler::apply(&self,
  glyph, cid)` (`apps/golemd/src/reconciler.rs:29`) sees one glyph at a time;
  there is no reconcile-scoped hook. The reconcile loop that *does* see all ops
  is `foreman::enact` (`apps/golemd/src/foreman.rs:158`), which iterates the
  planned `GlyphOp`s in order.
- **The surface constructors are reserved words** special-cased in
  `parser.rs::is_reserved_constructor` (`parser.rs:1009`) / `build_constructor`
  (`parser.rs:1028`), typed in `infer.rs` (`:1047`), lowered in `eval.rs`
  (`:141`) — not ordinary records. Any new field or policy spelling is new
  parser/`infer`/`eval` surface.

The modeling muse is ADR 0019: express the new capability as **typed attribute
sums on the existing glyph**, each variant carrying only its own fields, so
illegal states are unrepresentable — not as stringly optional fields, and not as
a fifth glyph.

## Decision

**Enrich `aptPackage` with two typed policy attributes — a `VersionPolicy` and
an `IndexPolicy` — both modeled as Elm-style sums on the existing glyph, and
collapse the per-glyph `apt-get update` into one refresh per reconcile.** The
glyph count stays four; `aptPackage { name }` keeps meaning exactly what it means
today.

### 1. `VersionPolicy` — pin a version, keep presence as the default

Add a `version` field to `AptPackage` whose type is a minimal sum, mirroring the
`Entry` arm style of ADR 0019:

```rust
pub enum Glyph {
    AptPackage { name: String, version: VersionPolicy, index: IndexPolicy },
    // …unchanged…
}

/// How tightly the installed version is controlled. `Present` is today's
/// presence-only behavior (any version satisfies the glyph); `Pinned` demands
/// an exact version string, installed as `name=version`.
pub enum VersionPolicy {
    Present,             // any installed version is acceptable (the default)
    Pinned(String),     // exactly this version; a different one installed is drift
}
```

- **`Present` is the default and is byte-for-byte today's behavior.** A bare
  `aptPackage { name }` lowers to `version = Present`. `apply_apt` under
  `Present` does exactly what it does now: install if `apt_installed` is false,
  no-op otherwise, never inspecting the version.
- **`Pinned(version)` installs `name=version` and treats a mismatch as drift.**
  `apply_apt` compares the *installed* version (from `dpkg-query -W
  -f=${Version}`) against the pin. Absent → `apt-get install -y name=version`.
  Installed-and-equal → no-op (`Inverse::Nothing`, `changed = false`).
  Installed-but-different → the reconciler installs `name=version` (apt upgrades
  or downgrades in place) and records an inverse that restores the prior version
  (`Inverse::RestoreAptVersion { name, prior_version }`), not a blanket remove.
- **The pin lives in the hashed glyph, so bumping it is a `Replace`.** Because
  `version` is a field on `AptPackage`, the content id (ADR 0012) now covers it.
  Changing `pinned "1.2.3"` to `pinned "1.2.4"` in source yields a new content
  id; the pure diff (`reconcile::plan`) emits `GlyphOp::Replace`; golemd reverses
  the old and applies the new (or, if `aptPackage` is registered as
  `replaces_in_place`, a single in-place apply whose captured inverse restores
  the old version — no window where the package is absent). **This is the
  deterministic, golem-native upgrade**: change the pin in source, re-apply. No
  imperative "upgrade" verb; the same content-addressed diff that upgrades every
  other glyph now upgrades apt packages.
- **`Latest` is deferred (recommended), not adopted — open for Dr. Dub.** A
  `Latest` arm would mean "track newest," reconciled by `apt-get install
  --only-upgrade name` on every apply. It is rejected as the *default* and
  recommended *deferred* because it breaks two core invariants:
  - **Determinism / content-addressing.** The same manifest would reconcile
    differently on different days — the desired state is no longer a concrete
    value the content id can pin. `Latest`'s content id is stable while the
    resulting host state is not, so the diff can never see the drift it causes;
    the manifest stops being a faithful description of the fleet (ADR 0012).
  - **Reversibility.** There is no fixed prior version to record as an inverse in
    the general case — reverse "the newest at some past apply" is ill-defined
    (ADR 0015).

  Deferring costs nothing structural: `Latest` is another arm of the same sum,
  addable later behind the same `format_version` machinery, so nothing here
  forecloses it. If Dr. Dub wants it now, add it explicitly labeled
  non-deterministic and document that a `Latest` glyph never participates in
  drift detection (it is always "apply, maybe changed") and records only a
  best-effort `prior_version` inverse. **Recommendation: ship `Present` +
  `Pinned`; defer `Latest`.**

### 2. `IndexPolicy` — the smart refresh (the real motivation)

Add an `index` field to `AptPackage` whose type is a sum controlling when the apt
index is refreshed:

```rust
/// When to refresh the apt index before resolving this package. Resolved once
/// per reconcile across all apt glyphs (§3), not per glyph.
pub enum IndexPolicy {
    Auto,                       // refresh iff sources changed since last update,
                                //   else iff the index is staler than ~24h (default)
    Always,                     // refresh unconditionally
    Never,                      // never refresh; trust the current index
    IfStale(DurationSecs),     // refresh iff the index is older than this
}
```

- **`Auto` is the default** and is the smart behavior the driver needs. It
  refreshes when either signal fires:
  1. **Sources changed since the last index update.** apt exposes no direct API
     for "did the sources change," so **compare mtimes**: the newest mtime under
     `/etc/apt/sources.list`, `/etc/apt/sources.list.d/*.list`, and
     `/etc/apt/sources.list.d/*.sources` versus the last index-update time. Read
     the update time from `/var/lib/apt/periodic/update-success-stamp` when it
     exists (apt's own success marker), falling back to the mtime of
     `/var/lib/apt/lists/`. If any source file is newer than the last update →
     refresh.
  2. **The in-reconcile signal — golem often *knows* it just wrote a source.**
     This reconcile's own plan is visible to the pre-pass (§3): if any
     `GlyphOp::Install`/`Replace` targets a `Filesystem` glyph whose `path` is
     under `/etc/apt/sources.list.d/` or is `/etc/apt/sources.list`, golem
     **forces a refresh** regardless of mtimes. This is the strongest, most
     direct signal — golem authored the change this cycle — and it closes the
     mtime race where the source file and the index-update stamp land in the same
     second. Mtime comparison is the fallback for sources golem did not write
     this reconcile (an operator edit, a prior reconcile).
  3. **Staleness fallback.** If neither of the above fires, refresh only if the
     index is older than a threshold (recommended **24h**), so a long-lived host
     still picks up upstream security updates without a refresh on every
     reconcile. **The exact threshold is an open sub-decision for Dr. Dub;** 24h
     matches Debian's default `APT::Periodic` cadence and is the recommendation.
- **`Always` / `Never` / `IfStale(d)` are the explicit escapes.** `Always` is the
  old per-install behavior on demand; `Never` trusts the current index (fast, for
  hosts managed out-of-band); `IfStale(d)` is `Auto`'s staleness rule without the
  sources-changed detection, for authors who want a pure time bound.
- **`Auto` is per-glyph in the type but resolved once (§3).** The policy is a
  glyph attribute so an author can annotate the *one* package that needs a fresh
  source, but the reconciler collapses all apt glyphs' policies into a single
  decision — see §3.

### 3. Reconcile-scoped refresh — one `apt-get update` per reconcile

The per-glyph `apt-get update` is the wasteful part, and `IndexPolicy` must not
reintroduce it as "up to one update per policy." Collapse it with a **pre-pass**
in the reconcile loop:

- **Where.** `foreman::enact` (`apps/golemd/src/foreman.rs:158`) already iterates
  all planned `GlyphOp`s in order and is the one place that sees the whole
  reconcile. Before the op loop, add a `refresh_apt_index_if_needed(&ops)`
  pre-pass.
- **How the policies collapse — most-aggressive wins.** Gather the `IndexPolicy`
  of every `aptPackage` glyph being installed or replaced this reconcile, then
  resolve one effective decision:
  - any `Always` present → **refresh**;
  - else run `Auto`'s detection **once** (the mtime comparison and the
    in-reconcile source-file signal of §2, computed a single time for the whole
    reconcile) — if it says refresh, **refresh**;
  - else if any `IfStale(d)` is satisfied by the current index age → **refresh**;
  - else (all `Never`, or every `Auto`/`IfStale` declined) → **skip**.

  At most one `apt-get update` runs, before any `apply_apt`. `apply_apt` no
  longer runs its own `apt-get update`.
- **The minimal hook.** The reconciler port stays per-glyph; the pre-pass is a
  reconcile-scoped concern that belongs in `foreman`, not smeared across the
  stateless adapter. Two shapes, recommend the first:
  1. **A reconcile-scoped method on the reconciler**, e.g.
     `Reconciler::prepare(&self, ops: &[GlyphOp]) -> EnactResult<()>`, called
     once by `foreman::enact` before the op loop. `HostReconciler::prepare`
     resolves the effective policy and runs at most one `apt-get update`; the
     `FakeReconciler` and non-apt reconcilers implement it as a no-op. This keeps
     the "reconciler owns host effects" boundary — `foreman` decides *when*, the
     reconciler decides *what command*.
  2. A shared per-reconcile flag (an `AtomicBool`/`Once` on `HostReconciler`) that
     `apply_apt` consults so only the first install refreshes. Rejected as the
     primary: it threads mutable shared state through a deliberately stateless
     adapter and cannot see the whole plan (so it cannot honor `Auto`'s
     "sources changed this reconcile" signal, which needs the full op list).
  The pre-pass refresh, being a read-mostly index update, records **no inverse**
  — refreshing the apt index is not a reversible desired-state change golem owns
  (see Alternatives §1); it is a precondition for resolving installs, exactly as
  the per-glyph update was.
- **Failure stays `Retryable`.** A failed pre-pass `apt-get update` fails the
  reconcile as `Retryable`, matching today's per-glyph behavior
  (`reconcilers.rs:76`).

### 4. The Emet surface

Keep the common case unchanged and add optional typed policy fields, spelled with
reserved lowercase policy constructors — mirroring how `file`/`directory`/
`symlink` are reserved constructors that build typed `Entry` arms (ADR 0019 §2):

```
aptPackage { name = "nginx" }
    -- version = Present, index = Auto — unchanged, the common case

aptPackage { name = "nginx", version = pinned "1.24.0-1" }
    -- exact pin; a bumped pin diffs to a Replace

aptPackage { name = "custom-agent", index = always }
    -- installed from a source this scroll just added; force a fresh index

aptPackage { name = "foo", version = pinned "2.1", index = ifStale (hours 24) }
```

- **`version` and `index` are optional fields defaulting to `present` / `auto`.**
  `build_constructor` (`parser.rs:1028`) is extended so `aptPackage` takes a
  required `name` and *optional* `version`/`index`, each defaulting when absent —
  a small departure from the current all-required `take_field` discipline, scoped
  to `aptPackage`. Existing `aptPackage { name }` programs parse and lower
  unchanged.
- **The policy constructors are reserved lowercase words that build typed sums**,
  the build/match split of ADR 0017: lowercase `present` / `pinned s` /
  `always` / `never` / `auto` / `ifStale d` *build*; PascalCase `Present` /
  `Pinned` / `Always` / `Never` / `Auto` / `IfStale` *match*. `VersionPolicy` and
  `IndexPolicy` become first-class Emet types alongside `AptPackage`
  (`infer.rs:1047`), so a library can compute a policy and pass it in. The
  duration for `ifStale` reuses a small `Duration` helper (`hours`/`minutes`),
  lowered to whole seconds on the wire.
- **`format_version` bumps `2 → 3`.** Adding the two fields (and their sums) to
  `AptPackage` changes the postcard layout; `check_format_version`
  (`manifest.rs:133`) cleanly rejects a v2 manifest rather than misparsing it.
  **A v2 `aptPackage { name }` and a v3 `aptPackage { name, version = Present,
  index = Auto }` are semantically identical** — the bump is purely the wire
  encoding of the two added fields, so the backward-compatible reading is exact:
  old bare `aptPackage` *means* `Present`/`Auto`. `emetc` and `golemd` upgrade in
  lockstep (they ship from one crate, ADR 0013); the disposable dogfood fleets
  re-init, as in ADR 0019 §5. The `key()` stays `apt:<name>` — one package per
  name is one resource, version and index are attributes of it, not identity.

## Alternatives considered

1. **A fifth `aptUpdate` / `aptSource` glyph** to model the index refresh as a
   declarative resource. **Rejected — it breaks "four kinds" and mismodels the
   thing.** Index freshness is not a piece of durable desired state golem
   reconciles toward and reverses; it is a *precondition* for resolving an
   install, transient and non-reversible (there is no "un-refresh"). A fifth
   glyph would need a `key()`, a diff identity, an `Inverse`, and a place in the
   reversal LIFO — none of which fit a stateless cache refresh. The refresh
   belongs *inside* the apt reconciler's reconcile-scoped pre-pass (§3), driven
   by a policy attribute on the package that needs it, exactly as the per-glyph
   update lives inside `apply_apt` today. This mirrors ADR 0019's ruling that a
   directory is a *variant of an existing glyph*, not a fifth kind: the new
   capability rides an attribute on an existing glyph, not a new reconciler.
2. **"Always latest" as the only (or default) version mode.** **Rejected —
   non-deterministic.** Making `Latest` the behavior would sever the content id
   from the host state it is supposed to describe: the same manifest would
   install different versions on different days, the diff could never see the
   drift it caused, and there would be no fixed prior version to reverse (§1). It
   defeats the two properties golem is built on — content-addressed determinism
   (ADR 0012) and exact reversibility (ADR 0015). `Present` (any version, but
   golem does not chase upgrades) plus `Pinned` (deterministic, content-addressed
   upgrades via a source edit) covers the real needs deterministically; `Latest`
   is deferred behind an explicit opt-in if ever wanted.
3. **Stringly optional fields** — `version: Option<String>`, `refresh:
   Option<String>` on `AptPackage`. **Rejected** for the ADR 0019 reason: an
   optional/stringly field readmits illegal and ambiguous states (`refresh =
   "sometimes"`, an empty-string version) and pushes validation to reconcile
   time. Typed sums make the choices exhaustive and checked in `emetc`, and let
   `IndexPolicy` carry structured data (`IfStale`'s duration) that a `String`
   could not.
4. **Keep the per-glyph `apt-get update`, add only `VersionPolicy`.**
   **Rejected as a half-measure:** the wire format must break for the `version`
   field regardless, so the `format_version` bump is already paid — folding in
   `IndexPolicy` and the reconcile-scoped refresh in the same break is nearly
   free and avoids a *second* bump later, and the index refresh is the stated
   real motivation (the new-source-then-install pattern), not a nice-to-have.
5. **Detect "sources changed" by hashing the sources files** instead of comparing
   mtimes. **Rejected as the mechanism, noted as a future hardening.** A content
   hash is more precise than an mtime (it ignores a touch that did not change
   content), but it needs somewhere to persist the last-seen hash across
   reconciles — new durable state golem would have to journal and version. The
   in-reconcile signal (§2.2 — golem knows it just wrote the source) already
   covers the case golem causes precisely, and mtime comparison covers the rest
   cheaply with no persisted state. Hashing is a later refinement if mtime races
   prove real.

## Consequences

- **golem can pin apt versions and upgrade them deterministically** — the
  content-addressed `Replace` that already upgrades every other glyph now upgrades
  apt packages: change the pin in source, re-apply. Version drift (host holds a
  different version than a `Pinned` glyph asks) is detected and corrected, with an
  inverse that restores the prior version rather than removing the package.
- **The new-source-then-install pattern works without wasting refreshes.** `Auto`
  refreshes when golem just wrote an apt source this reconcile, or when the
  on-disk sources are newer than the last index update, or when the index is
  stale past the threshold — and *only* then. The common steady-state reconcile
  runs zero `apt-get update`s instead of one per install.
- **One `apt-get update` per reconcile, at most.** The wasteful per-glyph refresh
  flagged at `reconcilers.rs:66` is retired; a reconcile-scoped pre-pass in
  `foreman::enact` resolves all apt glyphs' `IndexPolicy` into a single decision
  (most-aggressive wins) and runs at most one refresh. This needs the one minimal
  new hook the port lacks today — a reconcile-scoped `prepare(&ops)` the
  non-apt/fake reconcilers implement as a no-op.
- **A `format_version` bump (`2 → 3`)** and a lockstep `emetc`/`golemd` reship;
  the disposable dogfood fleets re-init rather than migrating manifests at rest.
  The anticipated evolution path (ADR 0012/0013), not an exceptional cost.
- **Backward-compatible by construction:** a bare `aptPackage { name }` *is*
  `version = Present, index = Auto`. Every existing program and every mental model
  of "install this package" is unchanged; the enrichment is opt-in per field.
- **The four-primitive invariant holds** — `aptPackage` gains attributes, not a
  fifth reconciler kind. Index freshness is (correctly) *not* a declarative
  resource; it is a policy on the package plus a reconcile-scoped effect, exactly
  as ADR 0019 kept directories inside the filesystem glyph.
- **New reverse surface is contained by the existing discipline:** a `Pinned`
  apply that changed an already-installed version records `RestoreAptVersion {
  name, prior_version }` and reverses to that exact prior version; golem still
  never removes or changes a package it did not install/change. The index refresh
  records no inverse (it is a precondition, not owned state).
- **Open sub-decisions flagged for Dr. Dub:** (a) **include `Latest` now, or
  defer?** — recommendation: defer (ship `Present` + `Pinned`), because `Latest`
  is non-deterministic and non-reversible and adds cleanly later behind the same
  bump; (b) **the `Auto` staleness threshold** — recommendation: 24h, matching
  Debian's `APT::Periodic` default; both are trivially adjustable and neither
  changes the wire shape (the threshold is a reconciler constant; `Latest` is one
  more sum arm).
- **Cross-references:** enriches the `aptPackage` glyph of ADR 0002 and its
  reconciler/`Inverse` from ADR 0015; a `Glyph` field change is pinned by
  `format_version` per ADR 0012/0013; the typed-attribute-sum modeling
  ("illegal states unrepresentable," policy as a minimal sum on the glyph)
  follows ADR 0019; the build/match constructor split for the policy sums follows
  ADR 0017.
