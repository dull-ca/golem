# 0020-write-ahead-reconcile-log

## Status

Accepted 2026-07-20; implementation to follow.

Refines ADR 0014 (the reconcile loop and journal) and ADR 0015 (reversible
reconcilers and content-addressed versioning). Neither is superseded: the pure
diff, the `Reconciler` port, the `Inverse` model, and content addressing all
stand. This ADR changes *when and how durability happens around* an enact — the
ordering of persistence relative to the side effect — and folds two ADR 0015
open items (rollback-vs-resume on partial failure; the applied-state snapshot)
into one write-ahead structure.

Narrowed by
[ADR 0057](0057-clearing-a-latched-failure-before-starting-a-unit.md)
(2026-08-11) in one place: §5's propagation pass runs `systemctl try-restart`
only for a unit that is **not** in systemd's `failed` state. A latched-failed
unit is cleared with `systemctl reset-failed` and then given a plain `restart`,
because `try-restart` against a latch is a successful no-op that starts nothing
— a green reconcile over a dead service, which is how it was found. "Restart
only if currently active; do nothing if not" below therefore now reads "do
nothing if it is merely inactive"; a failed unit is a third case §5 did not
distinguish. Everything else in §5 stands: the pass is still scoped to files
golem wrote under unit directories, the restart is still a `Restart`-action WAL
step outside the applied-set fold and outside rollback, and it is still
idempotently re-driven on recovery.

## Context

golemd's write path today (`apps/golemd/src/foreman.rs::reconcile`) is:

1. Read the prior applied state from the `PlanRoom`
   (`planroom.rs`, sqlite/memory over rusqlite).
2. `reconcile::plan(prior_outcomes, desired)` — the pure diff — emits one
   ordered `GlyphOp` per resource, keyed by `Glyph::key()` and versioned by
   content id: `Install` / `Noop` / `Replace` / `Remove`.
3. `enact(&ops)` runs every op through the `Reconciler` port with the retry
   spine, building an in-memory LIFO undo stack as it goes.
4. **Only if the whole plan succeeds**, overwrite the single-row `applied_state`
   snapshot and append one `Reconcile` `Revision` to the journal.

Two things are already right and must be preserved:

- **The per-glyph delta is already minimal.** A v2 scroll that changes one glyph
  produces exactly one `Replace` (or `Install`/`Remove`) and `Noop`s for every
  other resource; the `Noop`s touch nothing. There is no whole-scroll teardown.
  ADR 0015's alternative 3 already rejected whole-scroll re-apply. This ADR does
  not "fix" a churn problem in the diff — the diff is minimal. It refines only
  how one `Replace` is *mechanically enacted* (§4).
- **Reversal is a property of the (glyph, prior-state) pair, recorded — not
  recomputed** (ADR 0015). golem only ever reverses edits it recorded.

The forces that this ADR does address:

- **Durability is after-the-fact.** Persistence happens *after* the last op. A
  crash between "the reconciler mutated the host" and "the `Revision` was
  written" leaves golem with **no record of a side effect it already performed**.
  On restart, the prior applied state is stale: golem's memory says the glyph is
  at v1, the host is at v2, and no journal entry explains the gap. The captured
  `Inverse` for the half-done op is lost with the crashed process — so golem can
  no longer reverse exactly what it did. The `all-or-nothing`, in-memory undo
  stack only protects a *live* process; a killed process takes the undo stack
  with it.

- **Applied state is an overwriteable snapshot, not a log.** `put_applied_state`
  overwrites one row. A recent fix (`foreman::preserve_prior_inverses`, ADR 0015
  addendum) stopped a re-apply `Noop` from *clobbering* a glyph's real inverse
  with `Inverse::Nothing`, but the shape is still "latest snapshot wins." The
  snapshot cannot represent *"this reversal is half-done"*, so a crash mid-revert
  is indistinguishable from a completed one, and the next reconcile plans against
  a state that never actually settled.

- **Interrupted reversals can be clobbered.** A `Remove`/`Replace` that crashes
  after reversing the old version but before the new state is persisted leaves an
  in-progress reversal that the *next* manifest's reconcile will plan over — with
  no signal that a prior operation is unfinished.

- **The config-propagation gap.** A changed quadlet/unit `file` glyph whose
  `systemdService` unit name is unchanged is a `Replace` on the file and a
  `Noop` on the unit. `apply_systemd` short-circuits when the unit is already
  enabled+active (`reconcilers.rs::apply_systemd`, first branch), so the running
  container is **never restarted to pick up the new file**. The desired state
  changed; the running process did not converge.

Framed as unidirectional data flow (`lw:unidirectional-data-flow`): desired
state (the manifest) flows in → a pure diff folds it against recorded state into
an ordered plan → the plan is enacted with durable side effects → recorded state
is the fold's only output. The break in that flow today is that the *durable
write* is a single terminal step disconnected from the individual side effects
it is supposed to record. A message (`intended`) and its result (`done`/`failed`)
should be journalled *around each side effect*, so recorded state is always a
faithful, replayable fold of what actually happened — even across a crash. That
is a write-ahead log.

## Decision

### 1. A write-ahead log is the source of truth for "what is applied and how to reverse it"

Introduce a durable, append-only **write-ahead log (WAL)** in the same sqlite
database the `PlanRoom` already owns. The WAL records **intent before the side
effect and outcome after it**, per glyph op, so a restart can always see
in-flight work. It replaces the single-row `applied_state` snapshot as the
source of truth for the applied set and its inverses; the `Revision` journal is
derived from the WAL, not written independently of it (§6).

The unit of the WAL is the **step**: one `GlyphOp` being enacted (an `apply`, a
`reverse`, or the reverse-then-apply pair of a `Replace`), within one
**reconcile attempt** identified by a monotonically increasing `reconcile_id`.

#### Step lifecycle (states)

```
intended ──▶ done          (apply/reverse succeeded; outcome + inverse recorded)
   │
   ├───────▶ failed         (apply/reverse gave up or fatal; no state change claimed)
   │
   └───────▶ reversed       (a done step later undone — by rollback or a Remove/Replace)
```

- **`intended`** — written *before* the reconciler is called. Records what golem
  is about to do to which resource (op kind, key, target content id) and, for a
  step that will undo prior state (`reverse`/`Replace`/`Remove`), the `Inverse`
  it is about to consume (read from the prior `done` step — see recovery). An
  `intended` row with no terminal successor is the recovery signal.
- **`done`** — written *after* the reconciler returns `Ok`. Records the
  `Outcome`: the captured `Inverse` (for an `apply`), `changed`, and the content
  id now applied. This is the durable receipt ADR 0015 requires.
- **`failed`** — written after the retry spine gives up or hits `Fatal`. The
  step made no lasting claim; recovery treats the host as untouched by it (the
  reconcilers are idempotent and observe host state first, so a `failed` `apply`
  that partially ran is safe to re-drive or reverse).
- **`reversed`** — written when a previously `done` step is undone, either by
  in-attempt rollback or by a later attempt's `Remove`/`Replace`. A `done` step
  with no `reversed` successor is *currently applied*; its `Inverse` is live.

The **currently-applied set** (what `reconcile::plan` diffs against, replacing
`AppliedState.outcomes`) is a *derived view*: for each glyph key, the latest
`done` step with no subsequent `reversed` step. This view is a pure fold over the
WAL — the same fold recovery uses — so "the snapshot" is never written; it is
always computed. (A materialized `applied_state` row MAY be kept as a read cache,
rebuilt from the WAL on startup, but it is no longer authoritative.)

### 2. WAL schema sketch

Two new tables in the existing sqlite `PlanRoom` database (WAL journal-mode
already set). Bodies stay JSON, matching the current journal's
legibility-over-terseness choice (ADR 0014 §4 — the local store format is
golemd's private choice).

```sql
-- One row per reconcile attempt. Opened before planning, closed after the
-- attempt settles (committed or rolled back).
CREATE TABLE reconcile_attempt (
    reconcile_id      INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at        TEXT NOT NULL,
    scroll_content_id TEXT,               -- the manifest scroll being reconciled toward
    phase             TEXT NOT NULL,       -- 'planning' | 'enacting' | 'rolling_back'
                                           --  | 'committed' | 'rolled_back'
    settled_at        TEXT                 -- NULL while in flight
);

-- One row per step transition. Append-only; a step's history is its rows in
-- seq order. No UPDATEs — a state change is a new row.
CREATE TABLE wal_step (
    seq          INTEGER PRIMARY KEY AUTOINCREMENT,
    reconcile_id INTEGER NOT NULL REFERENCES reconcile_attempt(reconcile_id),
    step_ord     INTEGER NOT NULL,        -- position of this op within the plan
    glyph_key    TEXT NOT NULL,           -- Glyph::key() — the resource identity
    action       TEXT NOT NULL,           -- 'apply' | 'reverse'
    state        TEXT NOT NULL,           -- 'intended' | 'done' | 'failed' | 'reversed'
    op           TEXT NOT NULL,           -- the GlyphOp (JSON): kind + glyph + cids
    inverse      TEXT,                    -- JSON Inverse: intended-to-consume, or captured
    changed      INTEGER,                 -- outcome.changed, once 'done'
    at           TEXT NOT NULL
);
CREATE INDEX wal_step_by_attempt ON wal_step(reconcile_id, step_ord, seq);
```

Writes within one attempt are wrapped so that the `intended` row and the side
effect are ordered by fsync (`PRAGMA synchronous = NORMAL` on WAL mode already
guarantees the log record is durable before we proceed to the side effect,
provided each `intended`/`done` write is its own committed transaction — not one
transaction spanning the whole attempt, which would make nothing durable until
the end and reintroduce exactly today's problem). The invariant: **the
`intended` row is committed to disk before `Reconciler::apply`/`reverse` is
called, and the `done`/`failed` row is committed after it returns.**

### 3. Crash-recovery algorithm (startup)

On startup, before accepting any new manifest, golemd runs **recovery** over the
WAL:

1. Find the latest `reconcile_attempt`. If its `phase` is `committed` or
   `rolled_back`, there is nothing in flight — rebuild the currently-applied view
   (§1) and proceed normally.
2. If `phase` is `enacting` or `rolling_back`, the process died mid-attempt.
   Scan that attempt's `wal_step` rows in `step_ord` order and classify each:
   - **`done` with no `reversed`** — the side effect completed and is recorded;
     its `Inverse` is live. Leave it.
   - **`intended` with no terminal row** — the *dangerous* case: golem may or
     may not have performed the side effect (it crashed across the call). Resolve
     it by **re-driving idempotently**: because every reconciler observes host
     state first (`apt_installed`, `systemd_enabled/active`, `read_file`,
     `file_has_line`), re-running the `apply`/`reverse` converges whether or not
     the first call took effect, and re-captures the `Inverse`. The re-driven
     result is written as the step's `done`/`failed`.
   - **`failed`** — no claim; nothing to recover.
3. **Decide resume vs. roll back** for the interrupted attempt. The policy
   (matching ADR 0014/0015's all-or-nothing spine, now made crash-safe): an
   attempt in `enacting` that had not reached its last step is **rolled back** —
   set `phase = rolling_back` and reverse every `done` step of this attempt in
   reverse `step_ord`, writing each as `reversed`, then `phase = rolled_back`.
   The node returns to its last committed applied set. An attempt already in
   `rolling_back` is **resumed** — continue reversing its remaining `done` steps
   from where the WAL shows the rollback stopped; a rollback is itself a sequence
   of `reverse` steps with their own `intended`/`done` rows, so it resumes
   exactly, and a reversal is **never restarted from scratch or clobbered** (a
   `reversed` step is not reversed again).
4. Only once recovery reaches a terminal `phase` does golemd accept new
   manifests. A new reconcile **must not open** while an attempt is unsettled —
   this is what prevents a new manifest from overwriting an in-progress reversal
   (the failure mode called out in the brief). Recovery is a precondition of
   ingest, not concurrent with it.

Recovery is a pure fold over the WAL plus idempotent re-drives — the same
determinism as the reconcile itself (`lw:unidirectional-data-flow`): replaying
the logged messages yields the same settled state every time.

### 4. Minimal-delta reconcile: `Replace` in place where a glyph allows it

The diff is already minimal (§ Context). What is coarse is the *mechanics* of
`Replace`: today it is unconditionally `reverse(old)` then `apply(new)`
(`foreman.rs::enact`, `Replace` arm) — down-then-up for that one resource. For
some glyphs that stop/recreate is needless churn. Per glyph, honestly:

- **`file` — update in place.** Writing v2 over v1 is a single atomic
  temp-file-and-rename (`write_file_atomic`). The `Inverse` that reverses the
  *whole* `Replace` is `RestoreFile { prior v1 bytes+mode }`, which `apply_file`
  already captures. So a `file` `Replace` becomes **one `apply`** that records
  the pre-`Replace` contents as its inverse — no separate reverse step, no
  window where the file is absent. This is strictly better and loses nothing:
  reversal is still exact.
- **`lineInFile` — update in place.** v2 differs only if the line text changed
  (the key is `path`+intent). Reverse-then-apply here means "remove old line,
  add new line"; that is already two edits with no meaningful stop/recreate
  cost, but it can still be a single in-place rewrite capturing the prior line as
  the inverse. Low priority; the churn is negligible.
- **`aptPackage` — must reverse+apply.** The key is the package *name*; a
  `Replace` on the same name with a different content id means the desired glyph
  bytes changed. In practice the name is the whole glyph, so a true `Replace` is
  rare, but where it occurs an in-place `apt-get install` of a new version is
  *not* a general reverse+apply substitute (downgrade/pin semantics differ).
  Keep reverse-then-apply; do not special-case.
- **`systemdService` — must reverse+apply, and see §5.** A unit `Replace` (the
  unit name unchanged, glyph bytes changed) is unusual because the glyph is just
  the unit name. The interesting case is not a unit `Replace` at all — it is a
  `Noop` on the unit while its backing `file` changed. That is §5.

Rule of thumb recorded for future glyphs: **a glyph whose `apply` can atomically
overwrite the prior version and whose captured `Inverse` restores that prior
version can do `Replace` in place; a glyph whose reversal and re-application have
distinct externally-visible effects (a process stop/start, a package
remove/install) must stay reverse-then-apply.** The WAL represents an in-place
`Replace` as a single `apply` step whose `intended` records `old_cid → new_cid`
and whose `done` records the restore-to-v1 inverse — so recovery and rollback of
an in-place `Replace` are the ordinary single-step path.

### 5. Config-propagation: restart a unit when a file it depends on changes

**Decision: model the dependency in golemd's reconciler logic, keyed on the
directory the file lives in, as a post-reconcile restart pass — not a new glyph
kind and not (for now) an Emet-expressed edge.**

Rationale, weighed:

- A changed unit/quadlet `file` (a `Replace`, now in-place per §4) with an
  unchanged `systemdService` name must cause the unit to reload+restart, or the
  desired state (new config) is not actually running. This is a real dependency:
  *the unit depends on the file*.
- Expressing it as an Emet relationship (`unit restarts-on file`) is the
  "purest" model but pushes a golemd runtime concern into the language and the
  wire format, and there is no fifth resource kind to carry an edge — it would
  need a new manifest concept. Rejected for now as premature; the four-glyph
  contract stays closed (root `CLAUDE.md`).
- Deriving the dependency **structurally in golemd** needs no new model: a
  `file` glyph whose path is under a systemd unit directory
  (`/etc/systemd/system`, `/etc/containers/systemd` for Podman quadlets, and the
  drop-in dirs) is, by location, unit configuration. golemd already knows this
  coupling — `apply_systemd` runs `daemon-reload` precisely because a `file`
  earlier in the scroll may have written a unit (ADR 0015 addendum).

Mechanism: after the enact loop settles a reconcile, if **any `file` step in this
attempt was `changed = true` and its path is under a unit directory**, run a
**propagation pass**: `systemctl daemon-reload`, then for each affected unit
`systemctl try-restart <unit>` (restart only if currently active; do nothing if
not). "Affected unit" is resolved by mapping the changed file's path to its unit
name (a quadlet `foo.container` → `foo.service`; a `foo.service` file → itself;
a drop-in under `foo.service.d/` → `foo.service`). The restart is itself a WAL
step under a distinct `action = 'restart'` (its inverse `Nothing` — a restart of
a running unit has no separate reversal; the unit's enabled/active lifecycle is
still owned by the earlier `systemdService` step). The distinct action keeps the
bracket out of the applied-set fold (`wal::applied_outcomes` folds only `apply`
steps) and out of rollback, so a restart never registers its unit as applied — a
service that failed to enact still diffs as an attempt next reconcile rather than
being masked to a `Noop`. A crash during propagation recovers like any other
step, re-running the idempotent try-restart.

This keeps the unit's `Noop` honest — the *unit resource* genuinely did not
change — while closing the gap that its *inputs* did. It is deliberately scoped
to files golem itself wrote under unit directories, so it never restarts units
for host-managed config golem did not touch.

An Emet-expressed dependency is left as a **future refinement** (a follow-up ADR)
if the structural heuristic proves too coarse — e.g. a config file outside a
unit directory that a unit reads (`/etc/app/app.conf`). At that point the edge
becomes worth a first-class model. Recorded here as the known limitation, not
silently.

### 6. Relationship to `PlanRoom`, `Revision`, and the snapshot

- **`PlanRoom` is extended, not replaced.** It gains `reconcile_attempt` and
  `wal_step` and the recovery query surface; `applied_state` becomes a rebuildable
  cache (or is dropped in favor of the derived view). `MemoryPlanRoom` implements
  the same WAL semantics in memory so the whole spine stays testable with no disk
  (the ADR 0014 fake-adapter discipline).
- **`Revision` becomes a projection of the WAL, not an independently-written
  record.** A committed `reconcile_attempt` *is* a `Reconcile` revision; the
  `GET /revisions` surface (ADR 0014 §5) reads it from the settled WAL rather
  than from a separately-appended row. This removes the exact hazard this ADR
  exists to kill: there is no longer a window where the side effects happened but
  the `Revision` write did not. `Init` stays as the empty opening attempt.
- **`preserve_prior_inverses` is subsumed.** Its whole job (don't let a `Noop`
  overwrite a glyph's real inverse) disappears: a `Noop` writes **no new `done`
  step**, so the prior `done` step and its live inverse are simply still the
  latest for that key. The snapshot-clobber class of bug cannot occur in an
  append-only log. (ADR 0015's addendum documents the bug this replaces.)

## Alternatives considered

1. **Keep the after-the-fact snapshot + `Revision` (do nothing).** Rejected: the
   crash window between side effect and persistence is the core defect. Under it,
   golem can perform a host change it has no record of and no captured inverse
   for — violating ADR 0015's load-bearing property that golem can always
   reverse exactly what it did.
2. **Log outcomes, but still only after each op (no *intent* record).** Rejected:
   writing `done` after each op (instead of once at the end) shrinks the window
   but does not close it — a crash *between* the side effect and its `done` write
   still loses the inverse, and an `apply` that crashed mid-call leaves no signal
   that it was ever attempted. The *intent-before* record is what makes the
   in-flight step visible to recovery; without it there is nothing to resume.
3. **Full event-sourcing (the WAL is the only state; rebuild everything from the
   whole event log on every read).** Rejected as over-scoped. We adopt
   event-sourcing's *durability discipline* (append-only, intent-then-outcome,
   derive state by folding) for the reconcile path, but we do not make the entire
   daemon event-sourced, keep unbounded history hot, or forbid a materialized
   read cache. The WAL is compactable: once an attempt is `committed` and its
   steps are folded into the applied view, older superseded steps can be pruned
   behind a retained checkpoint. This is the ADR 0015-alt-4 lesson (don't reach
   for the heavy general mechanism when the scoped one suffices) applied to
   storage.
4. **Restart-on-change via a new glyph or Emet edge (for §5).** Rejected for now
   in favor of the structural heuristic — see §5 rationale — with the Emet edge
   recorded as the future refinement if the heuristic proves too coarse.
5. **Two-phase commit / distributed-transaction framing.** Rejected: there is one
   local durable store and one host; the problem is single-node crash
   consistency, which a local WAL with fsync-ordered intent/outcome solves
   without transaction-coordinator machinery.

## Consequences

- **No lost work across a crash.** Every side effect is bracketed by a durable
  `intended`→`done`/`failed` pair, so on restart golemd sees any in-flight step,
  re-drives it idempotently, and always holds the inverse needed to reverse it.
  The "performed but unrecorded" window is closed.
- **Interrupted reversals resume and are never clobbered.** A rollback is a
  sequence of WAL steps; recovery continues it from the logged point, and ingest
  is gated behind a settled attempt so no new manifest can overwrite an
  in-progress revert (brief items 1 and 3).
- **Applied state stops being a snapshot.** The currently-applied set and its
  inverses are a derived fold over an append-only log; the whole
  `preserve_prior_inverses` snapshot-clobber class of bug is structurally
  impossible.
- **`Replace` of a `file` stops churning.** One atomic in-place write replaces
  down-then-up, with no window where the file is absent and no loss of exact
  reversibility (brief item 4). `aptPackage`/`systemdService` stay
  reverse-then-apply — kept honest, not hidden.
- **Config changes propagate.** A changed unit-directory file triggers a
  `daemon-reload` + `try-restart` of the mapped unit, so a "v2 with one config
  tweak" actually restarts the running container (brief item 5) — scoped to files
  golem wrote under unit directories, never host-managed config.
- **sqlite schema/migration cost.** Two new tables plus a schema migration in
  `SqlitePlanRoom::open` (add-tables-if-not-exists is backward compatible; a
  one-time backfill converts the existing `applied_state` row + `revisions` into
  an initial `committed` attempt so history is preserved). `MemoryPlanRoom` gains
  the same semantics for tests. The wire format (`scroll-format`, postcard) is
  untouched — this is entirely golemd's private local store (ADR 0014 §4).
- **More writes per reconcile.** Two committed sqlite writes per op instead of one
  batch at the end. On WAL-mode sqlite with `synchronous = NORMAL` this is cheap
  relative to the host side effects (apt, systemctl, file writes) each op already
  performs; correctness under crash is worth the fsyncs.
- **What this forecloses:** the applied set can no longer be read as a single
  authoritative row without folding the log (mitigated by a rebuildable cache);
  and history is no longer free-growing without a compaction/retention policy
  (alternative 3) — a checkpoint-and-prune step becomes operational work golemd
  must own.
- **Cross-references:** refines ADR 0014 (`Revision` becomes a WAL projection;
  the retry spine and per-glyph diff are unchanged) and ADR 0015 (resolves its
  two open items — rollback-vs-resume on partial failure, now crash-safe resume;
  and the applied-state shape — while keeping the `Inverse` model and content
  addressing intact). The four-glyph contract (root `CLAUDE.md`) is unchanged: no
  fifth reconciler, no new resource kind — the config-propagation dependency is
  golemd reconciler logic, not a new glyph.

## Addendum — revisions projected, commit durability at FULL (implementation)

Two refinements the implementation settled on. Both tighten §2 and §6; neither
changes the decision or the HTTP contract.

### Revisions are projected, not stored

§6 leaves an out: "`Revision` becomes a projection … reads it from the settled
WAL." The implementation takes that literally — there is **no `revisions` table
and no `append_revision`**. `revisions`/`revision`/`latest_revision_id` on the
`PlanRoom` derive history from the `reconcile_attempt` and `wal_step` rows at read
time (`wal::projected_revisions`): revision 1 is `Init`, then one `Reconcile` per
`Committed` attempt, each attempt's outcomes folded from the WAL up to that
attempt's last step. `settle` marks the attempt `Committed` and reads the
projected latest revision back; it appends nothing.

This closes the projection window §6 gestured at. When a revision was a separate
appended row, a crash between "attempt committed" and "revision row written" could
leave a settled attempt with no revision — the same performed-but-unrecorded shape
this ADR exists to kill, one layer up. With the revision derived from the commit
itself, a settled attempt yields exactly one revision *by construction*; there is
no second write to lose. The `GET /revisions` surface (ADR 0014 §5) is unchanged —
same shape, same `Init` + one-per-reconcile numbering — only its source moved from
a stored row to a fold.

### Commit durability is `synchronous = FULL`, not NORMAL

§2 and the Consequences specify `PRAGMA synchronous = NORMAL`. The implementation
uses **`FULL`**. The bracketing invariant needs the `Intended` row *durable* before
the side effect runs, and "durable" here must mean survives power loss, not merely
a process crash. On WAL-mode sqlite, NORMAL acknowledges a commit before the WAL
frame is fsynced, so a power cut can lose an already-acknowledged `Intended` row —
reopening the exact "performed but unrecorded" window (§ Context, Consequences item
1) the log was built to close. FULL fsyncs each commit, so once `append_wal_step`
returns, the row is on disk. The cost is one fsync per WAL step (two per op) rather
than a deferred checkpoint; as §Consequences already argued, that is cheap next to
the host side effects each op performs, and crash-*and-power-loss* correctness is
worth it.
