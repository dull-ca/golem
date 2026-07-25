# 0031-recursive-scroll-grouping-and-failure-isolation

## Status

Proposed 2026-07-25.

Extends ADR 0009 (the per-host `Scroll` container) by making `Scroll` recursive,
and refines ADR 0029 (best-effort reconcile) by naming the **leaf-unit scroll**
as the unit of best-effort enact, retry, and rollback — replacing "the whole
host scroll" as that unit. Builds on ADR 0014 (the pure diff and reconcile loop),
ADR 0015 (reversible reconcilers, content-addressed versioning), and ADR 0020
(the write-ahead log). None is superseded: `Glyph::key()` identity, the pure
diff, the `Reconciler` port, the `Inverse` model, and the WAL bracketing
invariant all stand. The `format_version` bump here is the **same** `2 → 3` bump
ADR 0030 already introduces — both wire changes land together in one bump, not
two (see §5).

## Context

`main : List Scroll` today yields exactly two levels: a flat list of scrolls, one
per host (ADR 0009), each a flat `Vec<Glyph>` (`Scroll { name, glyphs }`,
`libs/scroll-format/src/scroll.rs:91`). A host is one scroll; a scroll is a bag
of glyphs. There is no level *between* "the host" and "an individual glyph" at
which to name a coherent thing that runs on the host — a Fishnet client, a
database, an ingress — as a group of the glyphs that constitute it.

ADR 0029 makes enact best-effort and gives it a retry budget and an `on_exhaust`
policy (`rollback` | `keep`). But it scopes all three — best-effort, retry, and
the rollback-vs-keep decision — to **the whole host scroll**: a single flat unit.
So an `on_exhaust = rollback` triggered by one exhausted glyph rolls back
*everything else golem applied this attempt*, across every unrelated thing on the
host. There is no boundary at which "the Fishnet analysis client couldn't
install; leave the move clients alone" can be expressed. Failure isolation and
grouping are the same missing concept viewed from two sides.

Forces:

- **A host runs many distinct units.** The motivating fleet: one box running
  "Fishnet Move Client #1..#3", "Fishnet Analysis Client #1..#2", and other
  units besides. Each unit is several glyphs (a package, a unit file, a config
  file, a service). The operator thinks in units, not loose glyphs, and wants
  status and failure reported that way.

- **Failure must not cross unit boundaries.** Move Client #2 failing or
  exhausting its retries must never roll back or block Move Client #1, Client #3,
  or the analysis clients. Today's flat scroll cannot express this.

- **Grouping is a container concern, not a reconciler concern.** golemd owns
  exactly four reconciler kinds (root `CLAUDE.md`); a group is inert structure,
  not a fifth thing an enact touches. Nothing inert may reach a reconciler.

- **The wire format is non-self-describing** (ADR 0012/0013): a type's field and
  variant order *is* the postcard encoding. Making `Scroll` recursive changes its
  layout — a `format_version` bump, not a free refactor.

- **Regrouping must not re-enact.** Glyph identity is `Glyph::key()` +
  per-glyph content id, position-independent (ADR 0015 §2). Moving a glyph into a
  differently-named group, or renaming a group, must change no glyph's identity
  and so must diff to nothing.

## Decision

**Make `Scroll` a recursive, strict tree. Every scroll — at any depth — is a
failure-isolation boundary carrying an optional retry/rollback policy. The leaf
scroll (one holding glyphs) is the unit of best-effort enact, retry, and
rollback. The four `Glyph` kinds are untouched: grouping is a container shape, not
a glyph.**

### 1. The recursive scroll — a strict tree

```rust
pub struct Scroll {
    pub name: String,
    pub policy: Option<Policy>,
    pub contents: Contents,
}

pub enum Contents {
    Glyphs(Vec<Glyph>),     // a LEAF scroll — a unit of glyphs
    Groups(Vec<Scroll>),    // a BRANCH scroll — named sub-scrolls, no loose glyphs
}
```

- **The tree is strict.** One level holds *either* glyphs *or* named
  sub-scrolls, never a mix. `Contents` is a sum, so a mixed level is
  unrepresentable — the ADR 0019 "illegal states unrepresentable" discipline,
  applied to grouping. An author who wants a loose glyph alongside sub-scrolls at
  the same level must wrap that glyph in its own named sub-scroll (a one-glyph
  leaf). This forecloses the ambiguous question "does this loose glyph belong to
  the group, before it, or after it?" — there are no loose glyphs at a branch
  level.

- **The top level is unchanged.** `main : List Scroll`, one **root scroll per
  host**. A "thing running on a host" and "a host" are the same concept at
  different depths: the root is the host, an interior branch is a subsystem, a
  leaf is a concrete unit of glyphs. The per-host root is still selected by
  `name` against `--host` (`foreman.rs:104`).

- **A flat scroll stays a leaf.** `scroll { name, contents = glyphs [ … ] }`
  (the common single-unit case) is a leaf and is itself one failure-isolation
  unit — the strict tree does not force a config to grow branches it does not
  need.

### 2. Every scroll is a failure-isolation boundary; the leaf is the unit

A **leaf-unit scroll** — a scroll whose `contents` is `Glyphs` — is the unit of:

- **best-effort enact** — its glyphs are enacted best-effort per ADR 0029 §1;
- **the retry budget** — the round-loop and limits of ADR 0029 §3 apply *within*
  the unit;
- **`on_exhaust`** — when the unit exhausts a limit with glyphs still failing, its
  `on_exhaust` (`rollback` | `keep`) applies to **that unit's subtree only**.

The load-bearing invariant: **one unit failing, exhausting, or rolling back never
rolls back, blocks, or otherwise touches a sibling unit.** Enact walks units in
source order (§4 of ADR 0029); each unit settles independently; a `rollback`
undoes only the glyphs golem applied *for that unit this attempt*. A branch scroll
is not itself enacted — it has no glyphs — it only groups and, via policy
cascade (§3), supplies defaults to the units beneath it.

### 3. Policy — retry knobs plus `on_exhaust`, cascading

`Policy` carries exactly the ADR 0029 §3 retry knobs and `on_exhaust`:

```rust
pub struct Policy {
    pub base_delay_ms:      Option<u64>,
    pub backoff_multiplier: Option<f64>,
    pub max_delay_ms:       Option<u64>,
    pub jitter_fraction:    Option<f64>,
    pub max_attempts:       Option<u32>,
    pub max_elapsed_ms:     Option<u64>,
    pub on_exhaust:         Option<OnExhaust>,   // Rollback | Keep
}
```

Every field is optional; an absent field inherits. Resolution for a leaf unit,
**nearest wins**:

1. `golemd.toml [retry]` — the fleet-wide default/fallback (ADR 0029 §3).
2. Each ancestor branch scroll's `policy`, root-to-leaf, each overriding the one
   above.
3. The leaf unit's own `policy`, overriding all ancestors.

A field unset at every level falls to the `golemd.toml` default, which itself
falls to the built-in default. **`on_exhaust` defaults to `rollback`** — the
safety default of ADR 0029 §4 (an attempt returns the unit to its last committed
state), with per-unit opt-out to `keep` for units that prefer forward progress
over atomicity. Policy lives *inside* the hashed scroll but **not** inside any
glyph, so it never perturbs a glyph's content id (§5).

### 4. Diff and group identity — diff stays on leaves

- **The diff is unchanged and still per-glyph.** `reconcile::plan`
  (`reconcile.rs:23`) keys by `Glyph::key()` and versions by per-glyph content id
  (ADR 0015 §2). Group structure is *not* an input to the diff. Flattening every
  leaf's glyphs into the desired set produces exactly the same ops regardless of
  how they are grouped, so **regrouping or renaming a group re-enacts nothing** —
  no glyph's key or content id depends on its enclosing scroll's name or depth.

- **Group identity is the name-path.** A unit's reporting identity is the path of
  `name`s from the root scroll to the leaf, e.g.
  `web / fishnet / analysis-client-2`. This path is *not* a glyph key and is not
  content-addressed — it is a reporting and policy-scoping label. Renaming a
  branch changes the path (and thus how the unit is reported and which policy
  cascade it inherits) but changes no glyph identity, so it triggers no
  re-enact.

- **A vanished unit's glyphs become removes under the parent's policy.** When a
  sub-scroll disappears between manifests, its glyphs are absent from the new
  desired set, so the diff emits `Remove` for each (`reconcile.rs:44`) — the
  existing teardown path. Those removes no longer have a leaf unit of their own
  (the unit is gone), so they run as a unit under **the surviving parent
  scroll's policy** — the nearest still-present ancestor in the name-path. The
  removes for one vanished unit are still isolated from sibling units; they are
  simply scoped to, and governed by, the parent that used to contain them.

### 5. Wire change — `format_version` 2 → 3, shared with ADR 0030

The recursive `Scroll` changes the postcard layout of `Scroll` itself and adds
`Contents`, `Policy`, and `OnExhaust`. This is a `format_version` bump. **It is
the same `2 → 3` bump ADR 0030 makes for the enriched `aptPackage` glyph** —
0030 and 0031 land together as *one* bump to `FORMAT_VERSION`
(`manifest.rs:20`), not two. `emetc` and `golemd` ship from one crate and upgrade
in lockstep (ADR 0013); the disposable dogfood fleets re-init rather than migrate
manifests at rest, as in ADR 0019 §5 and ADR 0030.

Because postcard is non-self-describing, field and variant order *is* the
encoding — spell it out once, and do not reorder later without another bump:

- **`Scroll`** — fields in order: `name: String`, `policy: Option<Policy>`,
  `contents: Contents`.
- **`Contents`** — variants in order: `Glyphs(Vec<Glyph>)`, then
  `Groups(Vec<Scroll>)`.
- **`Policy`** — fields in order: `base_delay_ms`, `backoff_multiplier`,
  `max_delay_ms`, `jitter_fraction`, `max_attempts`, `max_elapsed_ms`,
  `on_exhaust` (all `Option<…>`).
- **`OnExhaust`** — variants in order: `Rollback`, then `Keep`.

The per-scroll content id (`content_id`, `manifest.rs:76`) now covers `policy`
and `contents`, so a policy edit or a regrouping does change the *scroll's* id (it
is genuinely a different scroll) — but the per-*glyph* content ids the diff runs
on are unchanged, so no glyph re-enacts (§4).

### 6. golemd persists and reports the tree

- **The WAL records each op's unit.** `WalStep` (`journal.rs:230`) gains a
  `unit_path: Vec<String>` (the root-to-leaf name-path of the leaf unit the op
  belongs to); the `wal_step` table (`planroom.rs:166`) gains a `unit_path`
  column (serde_json, matching the existing `op`/`inverse` columns). Vanished-unit
  removes carry the surviving parent's path (§4). This is additive — the
  bracketing invariant, `step_ord`+`action` grouping, and the recovery fold (ADR
  0020 §3) are unchanged; the column is carried, not consulted, by recovery.

- **`ReconcileReport` (ADR 0029 §5) becomes tree-shaped.** It nests unit reports
  by name-path so `fleet apply` and `fleet status` render "what's on this box"
  grouped by unit — each unit's outcome (`settled` | `partial` | `rolled_back`)
  and its `GlyphFailure`s reported under its path. Every `GlyphFailure`
  (`journal`/`http.rs`) carries its `unit_path`. The exact nested shape is fixed
  in ADR 0029 §5 (revised alongside this ADR).

### 7. The Emet surface

Keep the flat case exactly as it is and add a `groups` constructor for the tree,
mirroring how `glyphs` already names a leaf's contents:

```
-- A leaf unit — the common case, unchanged in meaning (contents defaults to the
-- glyph list; see below):
scroll { name = "db", glyphs = [ aptPackage { name = "postgresql" }, … ] }

-- A branch grouping named leaf units, one host's root scroll:
scroll { name = "worker-01", groups =
  [ scroll { name = "fishnet-move",     groups =
      [ scroll { name = "client-1", glyphs = moveClient 1 }
      , scroll { name = "client-2", glyphs = moveClient 2 }
      , scroll { name = "client-3", glyphs = moveClient 3 }
      ] }
  , scroll { name = "fishnet-analysis", groups =
      [ scroll { name = "client-1", glyphs = analysisClient 1 }
      , scroll { name = "client-2", glyphs = analysisClient 2 }
      ] }
  , scroll { name = "base",  glyphs = baseline }
  ]
}

-- A per-unit policy override:
scroll { name = "client-2", policy = keep, glyphs = moveClient 2 }
```

- **`scroll` takes `name`, an optional `policy`, and exactly one of `glyphs` or
  `groups`.** `build_constructor` (`parser.rs:1061`) is extended: `glyphs` builds
  `Contents::Glyphs`, `groups` builds `Contents::Groups`, and supplying both — or
  neither — is a compile error (the strict-tree rule enforced at the surface,
  exactly as `build_constructor` already enforces the filesystem glyph's per-arm
  field set, ADR 0019 §2). The existing `scroll { name, glyphs }` programs parse
  and lower **unchanged** — `glyphs` still names a leaf's contents.

- **`policy` is optional and built by reserved lowercase policy constructors**,
  the build/match split of ADR 0017: lowercase `rollback` / `keep` *build* the
  `on_exhaust` choice; a record form
  `policy = retry { maxAttempts = 3, onExhaust = keep, … }` sets the retry knobs.
  Absent `policy` inherits (§3). (The exact spelling of the retry-knobs record —
  field names, whether durations reuse the `hours`/`minutes` helper of ADR 0030
  — is left to the implementing change; the decision here is that policy is an
  optional typed field on `scroll`, not stringly config.)

- **`Scroll`, `Contents`, and `Policy` gain first-class Emet types** alongside the
  existing `Scroll` type (`infer.rs:1946`), so a library can compute a group tree
  or a policy and pass it in. `main : List Scroll` is unchanged — the list is
  still one root scroll per host; the scrolls are now potentially deep.

## Alternatives considered

1. **A `group` glyph variant (grouping as a fifth `Glyph` kind).** Rejected —
   it breaks "four reconciler-owned kinds" (root `CLAUDE.md`) and mismodels the
   thing. A group is inert container structure; it has no host effect, no
   `apply`/`reverse`, no `Inverse`, and must never reach a reconciler. Grouping
   belongs to the *container* (`Scroll`), exactly as ADR 0019 kept `Directory`
   inside the filesystem glyph rather than minting a new kind. The recursion goes
   on the scroll, not the glyph.

2. **A loose (non-strict) tree — glyphs and sub-scrolls mixed at one level.**
   Rejected. It readmits the ambiguity the strict `Contents` sum forecloses: does
   a loose glyph beside three sub-scrolls belong to one of them, run before them,
   run after them, form its own implicit unit? A strict "glyphs xor groups" makes
   every unit explicit and named, at the cost of one wrapping scroll for a loose
   glyph — a small, honest naming ceremony that buys unambiguous failure scoping
   and reporting.

3. **Keep the flat scroll; express units with a naming convention on glyph
   keys.** Rejected — it puts group structure in a place the diff would have to
   parse back out of strings, gives no place to hang a per-unit policy, and makes
   "isolate this unit's failure" a string-prefix guess rather than a tree walk. A
   real level in the container is the honest home for a real level of the domain.

4. **Cross-unit dependency ordering (a DAG over units).** Rejected, consistent
   with ADR 0029 §6 and ADR 0020 §5. Units enact in **source order**; there is no
   edge kind and no scheduler. If unit B must come after unit A, write B after A.
   The tree expresses *grouping and isolation*, not *ordering dependencies* — a
   DAG would add cycle detection, a scheduler, and a new manifest concept to solve
   a problem source order already solves.

## Consequences

- **A host is a tree of named units, and failure is isolated per unit.** The
  motivating fleet — Move Clients #1..#3, Analysis Clients #1..#2, and a base
  unit — is one root scroll of named leaf units; one unit failing or rolling
  back leaves every sibling untouched. Status and failures report per unit,
  by name-path.

- **`format_version` bumps 2 → 3, shared with ADR 0030, and invalidates
  artifacts at rest.** A v2 manifest cleanly fails `check_format_version`
  (`manifest.rs:133`) rather than misparsing; the disposable fleets re-init. One
  bump carries both this recursion and 0030's `aptPackage` enrichment.

- **The strict tree imposes naming ceremony on small mixed configs.** A loose
  glyph beside sub-scrolls must be wrapped in a named one-glyph leaf. Mitigated:
  the flat `scroll { name, glyphs = [ … ] }` is still a valid leaf unit and needs
  no branches — the ceremony appears only when an author actually mixes levels.

- **The enact spine gains a tree walk.** `foreman::enact` (`foreman.rs:158`)
  walks the leaf units in source order and enacts each as an ADR 0029 best-effort
  unit; the round-loop and limits move *inside* the per-unit walk. Removes for a
  vanished unit run under the surviving parent's policy.

- **The WAL schema grows a `unit_path` field/column**, additive and not consulted
  by recovery. The `ReconcileReport` shape changes to a nested, per-unit tree
  (revised in ADR 0029 §5); `GlyphFailure` carries its unit path.

- **Regrouping and renaming are free at the glyph level.** Moving a glyph between
  groups or renaming a group changes the *scroll's* content id but no *glyph's*,
  so the diff yields no ops — golem re-enacts nothing.

- **What this forecloses:** no cross-unit dependency ordering beyond source order
  — the tree is grouping and isolation, not a DAG (ADR 0029's ordering contract
  stands). And "the whole host settles atomically" is no longer even the model:
  each unit settles independently, so the atomicity boundary is now the leaf unit,
  by design.

- **Cross-references:** extends ADR 0009 (the `Scroll` container becomes
  recursive), refines [ADR 0029](0029-best-effort-reconcile-retry-policy-and-structured-failure-reporting.md)
  (best-effort/retry/rollback scope becomes the leaf unit; the report becomes
  tree-shaped — revised there in step with this ADR), shares its `format_version`
  bump with [ADR 0030](0030-apt-package-version-and-index-policy.md), preserves
  the pure diff of ADR 0014, the content-addressed identity of ADR 0015, and the
  WAL bracketing of ADR 0020, and follows ADR 0017's build/match constructor
  split for the policy sums and ADR 0019's "illegal states unrepresentable" for
  the strict `Contents` sum. The four-glyph contract is unchanged — grouping is a
  container shape, not a glyph.
