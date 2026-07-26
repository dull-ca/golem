# 0034-within-host-execution-dedup-batching-parallelism

## Status

Proposed 2026-07-26.

An executor-internal optimization of the enact spine. It changes **how** golemd
runs a host's already-ordered work — deduping a glyph key declared by several
units, batching apt installs into one `apt-get install`, and running units
concurrently under a bounded pool — and changes **nothing** about the authored
model. Builds on ADR 0029 (best-effort per-unit enact, the round loop, the
structured report), ADR 0031 (the recursive scroll, leaf-unit isolation,
unit-scoped rollback), and ADR 0033 (async apply and the progress TUI that will
visualize the parallelism). Reuses the `Reconciler::prepare(&ops)` hook shape ADR
0030 proposes for its reconcile-scoped apt-index refresh — that hook, unbuilt
today, is built here and shared. None of 0029, 0030, 0031, 0033 is superseded:
the pure diff, the `Reconciler` port, the `Inverse` model, the WAL bracketing
invariant, and the tree-shaped report all stand.

**Dr. Dub's standing no-DAG ruling is honored and unchanged.** The authored model
has **no dependency graph** — source order is the whole ordering contract (ADR
0029 §6, ADR 0031 alternative 4). This ADR adds no authored edges and no
cross-unit ordering guarantees beyond a fixed three-phase executor shape (§3).
The direction it does act on is Dr. Dub's other request: *"if we have a bunch of
'install apt apps' we can do those ALL at once"* — collapse the redundant work an
already-ordered plan contains.

## Context

golemd's enact spine walks a host's leaf units **serially** and re-runs redundant
host commands, in three concrete places (`apps/golemd/src/foreman.rs`,
`reconcilers.rs`):

- **Units run one at a time.** `run_reconcile` (`foreman.rs:299`) iterates
  `desired.scroll.leaf_units()` in a `for` loop, calling `enact_unit`
  (`foreman.rs:486`) to completion — its whole best-effort round loop, backoff
  sleeps and all — before the next unit starts. A host of ten independent units
  drains them in series; a unit blocked in a two-minute `apt install` (the ADR
  0029 addendum measured `apt install podman` at 2m43s) stalls every later unit
  behind it, even ones that touch nothing it touches.

- **A glyph key shared across units is enacted once per unit.** Each unit is
  diffed independently against the **same, unchanging `prior`** applied set
  (`plan(&prior, &leaf_as_scroll(unit))`, `foreman.rs:318`) — `prior` is not
  mutated between units within an attempt. So if unit A and unit B both declare
  `apt:podman` (a base package several units depend on) and it is not yet
  applied, **both** diff it to `Install` and **both** call `enact_apply` →
  `apply_apt`. The first observes it absent, runs `apt-get update` + `apt-get
  install`, records `changed = true` with an `Inverse::RemoveAptPackage`. The
  second observes it now installed (`apt_installed` true, `reconcilers.rs:73`),
  short-circuits to `Inverse::Nothing`, `changed = false` — the
  idempotent-observation inverse. Correct, but the second unit's bracket is dead
  weight, and for apt the second `apply_apt` still pays its own `dpkg-query`
  probe.

- **apt refreshes and installs one package at a time.** `apply_apt`
  (`reconcilers.rs:72`) runs `apt-get update` (guarded by presence) then
  `apt-get install -y <name>` for **one** package. Ten apt glyphs are ten
  `install` invocations (and, today, up to ten `update`s — ADR 0030 collapses the
  refresh but not the install). `apt-get` can install many packages in one
  invocation and resolve their dependencies together; golem does not use that.

The forces:

- **Redundant host work is pure waste.** The same package installed by N units is
  one host change, not N. The second-through-Nth brackets exist only so each
  unit's report and WAL stay complete — the host effect already happened.

- **apt is the measured pain.** One `apt-get install pkg1 pkg2 …` resolves and
  fetches a whole set in a single dependency solve and a single mirror round-trip,
  far cheaper than N serial installs each re-locking dpkg. This is the specific
  win Dr. Dub asked for.

- **Independent units have no reason to wait.** Leaf-unit isolation (ADR 0031 §2)
  already says one unit's fate never touches a sibling's. Nothing in the *model*
  orders unit B after unit A unless the author wrote it that way — and the author
  cannot write a cross-unit order anyway (no DAG). Serial execution is an
  executor accident, not a contract.

- **The shared, non-reentrant host resources are the real constraint.** Two units
  installing packages contend on the **single dpkg lock** and the apt index; two
  units editing the same file via `lineInFile` race on that file; two units
  writing systemd unit files both run a **global** `systemctl daemon-reload`.
  `docs/TODO.md`'s parallel-apply line records within-host parallelism as gated
  precisely on serializing these (Dr. Dub, 2026-07-25). Concurrency is safe only
  where the reconcilers touch disjoint state.

What must be preserved exactly (the invariants this ADR is measured against):

- **Unit-scoped rollback semantics (ADR 0029 §4, 0031 §2).** `rollback_unit`
  (`foreman.rs:1037`) reverses only the steps whose `unit_path` equals the failing
  unit's, LIFO, via `next_reversible`. A sibling unit's applied steps are never in
  the set. This must hold unchanged under dedup and parallelism.

- **The shared-key rollback behavior as it stands today.** The crediting unit
  records a `changed = false` outcome with `Inverse::Nothing` (the second
  `apply_apt` returns exactly that). Reversing that outcome runs
  `reconciler.reverse` on `Inverse::Nothing` — a **no-op** (`reconcilers.rs:315`).
  So a crediting unit's rollback has **no host effect**; only the enacting unit's
  rollback (holding the real `RemoveAptPackage` inverse) actually removes the
  package. This is the invariant the dedup design must reproduce.

- **The WAL bracketing invariant and order-independent recovery fold (ADR 0020).**
  Every op is `Intended`→`Done`/`Failed`; the applied-set fold (`wal::applied_outcomes`)
  keys on `glyph_key` and picks the latest un-`Reversed` `Done`, and `cancelled_dones`
  pairs `Reversed`↔`Done` by `(step_ord, action, reconcile_id)`. `step_ord` is
  **attempt-unique** across all units (`enact_unit`'s shared `next_ord`,
  `foreman.rs:500`). The fold is a function of the row set, not of append order.

## Decision

Three changes to the executor, in the order they run. All three are internal to
golemd's enact spine; the manifest, the four glyphs, the report shape, and the
authoring contract are untouched.

### 1. Dedup a glyph key declared by several units

An identical `(key, cid)` declared by N units is enacted **once per attempt**. The
enacting bracket lands under the **first-declaring unit** in source order; the
other N−1 units record a **credited bracket** identical to what today's
second-`apply_apt` re-observation produces — `Done`, `changed = false`, inverse
`Inverse::Nothing`, its own `step_ord` and `unit_path`.

- **Scope of "identical" is `(key, cid)`.** Same resource identity *and* same
  content id. The enacting unit runs the real `apply`; the crediting units skip
  the reconciler call and append the credited `Done` directly. Every unit still
  gets a bracket per its glyph, so per-unit reports and the WAL stay complete —
  the diff and the applied-set fold are unchanged.

- **Rollback semantics are unchanged — this is the load-bearing point.** A
  crediting unit's credited bracket is `changed = false` / `Inverse::Nothing`, so
  a unit-scoped rollback of that unit reverses a no-op — **zero host effect**,
  exactly as reversing today's re-observation outcome does (`reverse` on
  `Inverse::Nothing`). The enacting unit's rollback behaves exactly as today: it
  holds the real inverse (`RemoveAptPackage`, `RestoreFile`, …) and its
  `rollback_unit` removes/restores the resource. So: **the unit that did the host
  work owns the undo; a crediting unit owns nothing to undo.** This is the precise
  behavior the pre-dedup code already exhibits — dedup makes it deliberate and
  cheap (skips the redundant reconciler probe) rather than accidental (a second
  full `apply` that happens to observe idempotence). Recorded as the preserved
  invariant: *dedup changes which unit runs the reconciler, never which unit can
  reverse a host change.*

- **Divergent cids for one key across units — the recorded truth and a surfaced
  wart.** ADR 0031 §4 makes the same key across sibling leaves legal. But if two
  units declare the same key with **different content** — `file /etc/x` with body
  "a" in unit A and body "b" in unit B — the current whole-flatten diff
  (`reconcile::plan` over `desired.all_glyphs()`, and the per-unit
  `leaf_as_scroll` diff) does **not** reconcile or reject the conflict: each unit
  diffs independently against `prior`, so **both** ops run, and the **last unit in
  source order wins** the on-host state (it overwrites what the earlier unit
  wrote). This is **silent last-wins across units today**, and it is a latent
  correctness wart, not a decision this ADR makes. Dedup keys on `(key, cid)`, so
  divergent cids are **not** deduped — they remain two distinct ops that still
  race to last-wins. **Recorded as a known wart to surface, not silently
  resolved:** golemd should **detect** two units declaring the same `key` with
  divergent `cid` in one attempt and **report** it (a warning event on the
  progress ring per ADR 0033 §2, and a note in the report) so an author sees the
  conflict rather than debugging a mysteriously-overwritten file. Whether to
  additionally make it a compile-time `emetc` error is left open (§ Open
  questions) — the analyze-time model has no cross-unit conflict check today, and
  adding one is a separate decision from this executor change.

### 2. Batch apt installs into one invocation, with a per-glyph fallback

All apt `Install` ops across the attempt (after dedup — one per package) collapse
into a single `apt-get install pkg1 pkg2 …`, run **before** the per-unit enact
phase through the ADR 0030-shaped hook:

```rust
fn prepare(&self, ops: &[GlyphOp]) -> EnactResult<PrepareOutcome> { … }
```

- **Where.** `run_reconcile` (`foreman.rs:299`) calls `reconciler.prepare(&ops)`
  once, over the whole attempt's planned ops, **before** the unit loop —
  the same reconcile-scoped pre-pass slot ADR 0030 §3 defines for its index
  refresh. `HostReconciler::prepare` gathers the distinct apt package names from
  the `Install` ops and runs one `apt-get install -y pkg1 pkg2 …`; the non-apt and
  fake reconcilers implement it as a no-op (default trait method). `prepare`
  returns a `PrepareOutcome { batch_installed }` — the names that were **absent on
  the host before** the batch and whose install (batch or per-glyph fallback)
  **succeeded** — the receipt the foreman needs to attribute the batch's host
  effect per unit (see the next bullet). ADR 0030's `apt-get update` index refresh,
  when built, runs in this **same** `prepare` hook, before the batched install —
  one pre-pass, two responsibilities.

- **Per-unit brackets are recorded from the batch result, via a foreman claim.**
  The batch is a host effect that happened before any unit's bracket, so a per-unit
  `apply_apt` can no longer tell "golem batch-installed this package this attempt"
  from "it pre-existed": it observes the package **already installed**
  (`apt_installed` true) and truthfully records `changed = false` /
  `Inverse::Nothing` — for **every** declarer, leaving no unit holding the real
  `RemoveAptPackage`. The foreman closes that gap without teaching the stateless
  `apply_apt` any attempt state: it seeds an attempt-scoped **claim set** from
  `PrepareOutcome.batch_installed`, and the **first** unit to reach each such
  package **claims** it — recording the `RemoveAptPackage` bracket
  (`changed = true`) **without re-running apt**, since the observation (absent
  before, present now, by the batch) is the claim. Later declarers of a shared
  package credit (`changed = false` / `Inverse::Nothing`) exactly as dedup (§1)
  does, because the claim also enters them in the success set. So the reverse
  inverse is still per-package `RemoveAptPackage` held by exactly one unit, and
  **rollback stays per-unit and per-package** — an `on_exhaust = rollback` of the
  claiming unit removes the batch-installed package, and the batch install does not
  create a batched, unsplittable undo. A package absent-before whose install
  **failed** is deliberately not in `batch_installed`, so its later per-unit apply
  records the real inverse the ordinary way — no double-claim.

- **Failure attribution and the correctness-preserving fallback.** `apt-get
  install pkg1 pkg2` fails the **whole** invocation if any one package is
  unresolvable — apt gives no clean per-package success split. So on a **batch
  failure**, `prepare` **falls back to per-glyph installs**: it runs `apt-get
  install -y <name>` for each package individually, so one bad package fails only
  its own glyph and its siblings' packages still install. The batch is an
  **optimization with a correctness-preserving fallback**, never a new failure
  mode: the worst case is exactly today's per-package behavior, reached only when
  the batch does not resolve cleanly. Fallback failures classify as today —
  `Retryable` (`reconcilers.rs:85`) — so the round loop retries the individual
  glyph.

- **Removes are NOT batched.** Package removes are rare and ordering-sensitive
  (a remove teardown unwinds in reverse-of-apply order, ADR 0029 §6), and the
  reverse path is already per-glyph via `Inverse::RemoveAptPackage`. Batching them
  would tangle the LIFO rollback for no measured gain. Removes stay per-glyph, in
  the post-unit removes phase (§3).

### 3. Bounded parallelism across units, per-kind serialization within

After the batch phase, units enact **concurrently** on a bounded worker pool
(default 4, configurable), each unit keeping its own internal source-order round
loop. Cross-unit removes run after, serially.

- **The pool.** A fixed worker pool of `[enact] workers` (default 4) drains the
  unit queue; each worker runs one unit's `enact_unit` to completion (its whole
  best-effort round loop, its `on_exhaust`) before taking the next. Within a unit,
  source order is unchanged — glyphs still enact in order, installs/replaces then
  the unit's own removes. `workers = 1` reproduces today's fully-serial behavior
  exactly, the safe fallback.

  ```toml
  # golemd.toml — golemd's private operational surface (never on the wire).
  [enact]
  workers = 4     # bounded concurrent units; 1 == serial (today's behavior)
  ```

  This `[enact]` table sits beside `[retry]` (`config.rs`) — both are golemd's
  operational config, not the manifest wire contract, and not the per-scroll
  `policy` (which is on the wire, ADR 0031 §5).

- **Per-kind serialization — the honest lock granularity, analyzed from the
  reconcilers as written (`reconcilers.rs`):**
  - **apt / dpkg — globally serialized.** One dpkg lock on the host; two concurrent
    `apt-get` invocations collide. A single **apt mutex** in the reconciler
    serializes every apt command (install, remove, the ADR 0030 refresh). The batch
    (§2) front-loads most apt work into the single-threaded pre-pass, so this lock
    is contended rarely during the parallel phase (only removes and any fallback
    retry).
  - **`lineInFile` — serialized per target file.** Two units appending to the same
    file race on read-modify-write (`apply_line_in_file` → `file_has_line` +
    `append_line`, `reconcilers.rs:705`). A **per-path mutex** (keyed on the
    `lineInFile` path) serializes writers to one file; distinct files proceed
    concurrently. `lineInFile` on distinct paths needs no lock.
  - **systemd — a global `daemon-reload` lock, but concurrent per-unit
    enable/start.** This is the subtle one. `apply_systemd` (`reconcilers.rs:126`)
    and `try_restart` (`reconcilers.rs:242`) each run `systemctl daemon-reload` —
    a **global** systemd operation that reprocesses **all** unit files and runs
    generators — before enabling/starting **their** unit. `systemctl` itself
    queues concurrent reloads safely, but the hazard is a TOCTOU across units: unit
    A writes a quadlet `file` then daemon-reloads; unit B, reloading concurrently,
    can trigger a generator pass while A's file is mid-visibility, and two reloads
    racing waste a full reprocess each. **Recorded granularity: serialize
    `daemon-reload` under one global systemd mutex** (a reload runs alone), but
    **`enable`/`start`/`stop`/`try-restart` of *distinct* units need no lock** —
    they touch independent unit state and systemd handles them concurrently. So the
    systemd adapter takes the global lock only around `daemon-reload`, then releases
    it for the per-unit enable/start. Within a single unit, its `file` glyph
    already precedes its `systemdService` glyph in source order (the unit writes its
    quadlet before starting it), so no unit reloads before its own file is written;
    the lock only orders the *global* reprocess between units.
  - **filesystem (`file`, `directory`, `symlink`) — unrestricted.** Distinct paths
    are independent; the writes are atomic temp-and-rename (`write_file_atomic`).
    Two units writing the **same** file path is the divergent-cid wart of §1
    (surfaced, not locked). No lock added for the common disjoint-path case.

- **Concurrency of the shared executor state — the types that must change.**
  - `step_ord` allocation becomes **atomic.** Today `enact_unit` takes `next_ord:
    &mut u64` and does `base_ord = *next_ord; *next_ord += ops.len()`
    (`foreman.rs:500`) — a non-`Send` mutable borrow that cannot cross threads.
    It becomes an `AtomicU64` (or a mutex-guarded counter) so each unit reserves a
    disjoint `[base_ord, base_ord+len)` block atomically. Attempt-uniqueness of
    `step_ord` — which `has_terminal`, `next_reversible`, `reversed_after`, and
    `wal::cancelled_dones` all depend on — is preserved: reserved blocks never
    overlap.
  - **The shared retry clock becomes thread-safe.** Today `retry_clock:
    Cell<Option<Instant>>` (`foreman.rs:312`) is `!Sync`. It becomes an
    `AtomicU64`/`Mutex<Option<Instant>>` (or a `OnceCell`-style guarded set) so the
    "budget starts at the first retry decision, shared attempt-wide" semantics of
    ADR 0029 §3 hold across concurrent units — the first unit to reach a retry
    decision sets the clock, later units read the same shared start. The
    attempt-wide `max_elapsed_ms` budget stays one shared clock, now readable from
    N workers.
  - **The WAL append path is already per-query locked.** `SqlitePlanRoom` guards
    each `append_wal_step` behind its own `conn` mutex (ADR 0033 §2, concurrency
    stance), releasing between calls, so concurrent workers appending brackets
    serialize on the connection for the write alone and interleave otherwise. No
    change needed — but brackets from different units now **interleave** in `seq`
    order (see Consequences).
  - **The reconciler is already `Send + Sync`** (`reconciler.rs:28`), and
    `PanicCatching` contains a panic per call, so a panicking glyph on one worker
    is a `Fatal` for that unit and never unwinds across the pool.
  - **Events/progress is already concurrent-safe.** `ProgressRegistry` guards its
    rings behind one `Mutex` (`progress.rs:73`); `record`/`set_retry`/`clear_retry`
    all take it. N workers recording concurrently is safe as-is; the `seq` cursor
    stays monotone. **The TUI needs no change** (ADR 0033 §3) — it already renders
    a tree of per-unit nodes with independent spinners, so N units settling out of
    order and spinning at once is exactly the shape it was built for.

- **Cross-unit removes stay after the unit phase, serial.** The vanished-unit
  removes (`plan_vanished_removes`, `foreman.rs:418`) run after all units settle,
  in one serial pass, unchanged — teardown ordering (reverse-of-apply, ADR 0029
  §6) matters and the volume is small.

### 4. What this is NOT

- **No authored dependency edges, no DAG.** The manifest gains no edge kind; Emet
  gains no `after`/`dependsOn`. Dr. Dub's standing ruling — the authored model has
  no dependency graph, source order is the whole contract (ADR 0029 §6, ADR 0031
  alternative 4) — is **unchanged**.
- **No cross-unit ordering guarantee** beyond the fixed executor shape: **batch
  phase → parallel unit phase → removes phase.** Within a unit, source order
  holds. Between units in the parallel phase there is **no** order — which is sound
  precisely because the model already promises none across units, and unit
  isolation (ADR 0031 §2) means none is needed.
- **Not a per-glyph scheduler.** The parallel grain is the **unit**, not the
  glyph; a unit's glyphs stay in-order and single-threaded within the unit.

## Alternatives considered

1. **A full DAG scheduler over glyphs or units.** Rejected — it reopens the
   settled authored-ordering decision. A DAG needs an edge kind on the wire, cycle
   detection, a scheduler, and a new manifest concept, to solve an ordering problem
   **source order already solves** (ADR 0029 §6, ADR 0031 alternative 4, ADR 0020
   §5 — all reject a dependency graph for the same reason). This ADR's parallelism
   is *executor-internal* and rides the isolation the tree already guarantees; it
   introduces no authored ordering and so does not touch that decision. Adopting a
   DAG would be a reversal of Dr. Dub's ruling, not an extension of it.

2. **Per-kind global phases beyond apt (all filesystem, then all systemd, …).**
   Rejected for now. apt is the **measured** pain (the 2m43s cold install, the
   per-package dpkg re-lock), and batching it is the specific win Dr. Dub named.
   Phasing every kind globally would break per-unit reporting locality (a unit's
   glyphs would no longer settle together) and per-unit rollback grouping for a
   speedup nothing has measured. The unit stays the parallel grain; only apt gets a
   cross-unit batch, because only apt's per-invocation overhead justifies it.

3. **Unbounded parallelism (one thread per unit).** Rejected. A host with many
   units would spawn many concurrent `apt`/`systemctl` processes, all contending on
   the one dpkg lock and thrashing a small VM, and flood the journal and the
   progress ring with interleaved output. A **bounded** pool (default 4) caps
   contention and keeps the TUI legible; `workers = 1` is the serial escape hatch.

## Consequences

- **Redundant host work collapses.** A package declared by N units installs once
  (batched, §2), and shared-key glyphs enact once (§1); the other units credit a
  cheap `changed = false` bracket instead of re-probing the host. The report and
  WAL stay complete per unit.

- **apt is dramatically cheaper on a cold host.** One `apt-get install pkg1 pkg2
  …` resolves and fetches the whole set in one dependency solve and one mirror
  round-trip, versus N serial installs each re-locking dpkg — with a per-glyph
  fallback so a single unresolvable package never fails its siblings' installs
  (§2). Worst case equals today's per-package behavior.

- **Independent units run concurrently.** A slow `apt install` in one unit no
  longer stalls unrelated units behind it; the bounded pool (default 4) overlaps
  their work, gated by per-kind locks so the shared dpkg lock, per-file
  `lineInFile` writes, and the global systemd `daemon-reload` never race.

- **WAL ordering changes: brackets from different units interleave in `seq`, and
  the recovery fold is unaffected.** With concurrent workers, `seq` order no longer
  groups a unit's brackets contiguously. This is **safe** because the applied-set
  fold (`wal::applied_outcomes`) and rollback pairing (`cancelled_dones`,
  `next_reversible`) key on `glyph_key` and `(step_ord, action, reconcile_id)` —
  **not** on append order — and `step_ord` stays attempt-unique via the atomic
  allocator (§3). Recovery re-drives every un-terminated `Intended` and rolls back
  every un-`Reversed` `Done` regardless of the order they were appended; the fold
  is order-independent per key. `rollback_unit` still filters by `unit_path`, so
  interleaving does not leak one unit's steps into another's rollback set. Verified
  against the fold and pairing logic in `wal.rs` — no change required there.

- **Report and TUI: units settle out of order, which is fine.** The report's
  `units` list is still emitted in source order (`ReconcileReport::roll_up`,
  `report.rs:142`) regardless of completion order; the TUI (ADR 0033 §3) already
  renders a per-unit tree with independent spinners, so parallel spinners and
  out-of-order settling are the shape it was built for. No TUI change.

- **The divergent-cid wart is surfaced, not silently decided.** Two units
  declaring the same `key` with different `cid` in one attempt still resolve
  **last-wins on the host today** (each unit diffs independently against `prior`);
  dedup keys on `(key, cid)` and so does not touch that path. golemd will now
  **warn** on the conflict (a progress event + report note) so an author sees it,
  rather than debugging a file that one unit silently overwrites. Making it an
  `emetc` compile error is left open.

- **New concurrency surface in the executor, contained.** `step_ord` allocation
  and the shared retry clock move from `&mut`/`Cell` to atomic/mutex-guarded; the
  reconciler gains an apt mutex, a per-path `lineInFile` mutex map, and a global
  systemd `daemon-reload` mutex. The WAL append path and the progress ring are
  already mutex-guarded and need no change. The blast radius is the enact spine and
  the reconciler's shared-resource guards — not the diff, the port, the WAL fold,
  or the report shape.

- **ADR 0030's implementation becomes easier.** The `Reconciler::prepare(&ops)`
  hook 0030 proposed for its reconcile-scoped index refresh is **built here** and
  shared: 0030's `apt-get update` slots into the same pre-pass, before the batched
  install, when 0030 lands. One hook, two reconcile-scoped responsibilities.

- **What this forecloses:** the enact spine is no longer serial, so any future
  logic assuming "units run one at a time" or "brackets append in unit-contiguous
  `seq` order" must not be reintroduced. And the parallel grain is fixed at the
  **unit** — a future need for *intra-unit* glyph parallelism would reopen §3 (and
  would still owe the same per-kind locks).

- **Cross-references:** optimizes the enact spine of ADR 0029 (the per-unit
  best-effort round loop is unchanged, now run concurrently under a pool),
  preserves ADR 0031's leaf-unit isolation and unit-scoped rollback (dedup and
  parallelism both respect `unit_path`), reuses ADR 0030's `prepare(&ops)` hook for
  the apt batch and shares it with 0030's index refresh, and relies on ADR 0033's
  concurrent-safe progress ring and per-unit TUI (both already built for this) and
  its per-query WAL locking. The four-glyph contract, the manifest wire format, and
  the no-DAG authored-ordering ruling (Dr. Dub; ADR 0029 §6, ADR 0031) are all
  unchanged — `[enact] workers` is golemd's private operational surface, like
  `[retry]`.

## Open questions

- **Divergent-cid conflict: warn-only, or an `emetc` error too?** This ADR decides
  golemd surfaces the runtime conflict (warn + report note). Whether `emetc` should
  additionally reject two units declaring the same `key` with divergent `cid` at
  compile time is a separate analyze-time decision — the model has no cross-unit
  conflict check today, and adding one touches the diff/analyze path, not this
  executor change.

- **Default worker count.** `4` is the proposed default; the right number is a
  function of typical host size (cores, whether it is a small VM) and how much of
  the work the apt batch already front-loads. Trivially adjustable
  (`[enact] workers`), and `1` is always the safe serial fallback.

- **Whether the systemd `daemon-reload` lock should also cover the immediately
  following `enable`.** The analysis serializes `daemon-reload` alone and lets
  per-unit `enable`/`start` run concurrently. If a real host shows a
  reload-then-enable race for freshly-generated units, the lock window may need to
  extend through the enable of a unit whose file this attempt just wrote. Left to
  the first real concurrent dogfood run to confirm.
