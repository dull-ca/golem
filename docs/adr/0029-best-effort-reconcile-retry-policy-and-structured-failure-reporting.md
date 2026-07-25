# 0029-best-effort-reconcile-retry-policy-and-structured-failure-reporting

## Status

Accepted 2026-07-25 (implemented). Proposed 2026-07-24. Revised 2026-07-25 to scope best-effort, retry, and
`on_exhaust` to the **leaf-unit scroll** of
[ADR 0031](0031-recursive-scroll-grouping-and-failure-isolation.md) rather than
the whole host scroll, and to make the report tree-shaped. 0031 makes `Scroll`
recursive and every scroll a failure-isolation boundary; this ADR is what those
boundaries *do* at enact time.

Refines ADR 0014 (the reconcile loop and journal), ADR 0015 (reversible
reconcilers and content-addressed versioning), and ADR 0020 (the write-ahead
reconcile log), and works in step with ADR 0031 (recursive scroll grouping). None
is superseded. The pure diff, the `Reconciler` port, the `Inverse` model, content
addressing, and the WAL bracketing invariant all stand. This ADR changes *how the
enact loop treats a failing op* (best-effort instead of fail-fast), *at what scope
it retries and rolls back* (per leaf unit, not per host scroll), *where the retry
policy lives* (a golemd config file, overridable per scroll, instead of two
hardcoded constants), and *what golemd returns when a reconcile does not fully
settle* (a structured, tree-shaped report instead of one raw error string). The
WAL, its recovery algorithm, and reversibility-as-a-guarantee are preserved and
reused. The retry-policy config file here is the fleet-wide default that ADR
0031's per-scroll `policy` cascade overrides.

## Context

golemd's write path today, in `apps/golemd/src/foreman.rs`:

- **Fail-fast on the first error.** `Foreman::enact` (`foreman.rs:158`) runs the
  planned ops in order; each `enact_apply`/`enact_reverse` propagates its error
  with `?`, so the **first** failing op aborts the whole loop
  (`foreman.rs:171`, `179–180`, `186`). `Foreman::reconcile` (`foreman.rs:135`)
  then calls `rollback_attempt` (`foreman.rs:423`, LIFO reversal of this
  attempt's still-applied steps), marks the attempt `RolledBack`, and returns one
  `anyhow::Error` (`foreman.rs:140–145`). A scroll with two unrelated failing
  glyphs reports only the first; the rest are never attempted.

- **Retry is per-op, with two hardcoded constants.** `Foreman::attempt` and
  `Foreman::attempt_reverse` (`foreman.rs:456`, `473`) loop `1..=self.max_attempts`
  (`max_attempts: 5`, `foreman.rs:67`) sleeping a fixed `retry_delay` of `200ms`
  (`foreman.rs:68`) between tries, only on `EnactError::Retryable`;
  `EnactError::Fatal` (`reconciler.rs:16`) bails at once (`foreman.rs:460`,
  `477`). There is **no backoff, no jitter, no wall-time bound, and no
  configurability** beyond the test-only `with_retry` (`foreman.rs:77`). The two
  spines are duplicated (`attempt` for `apply`, `attempt_reverse` for `reverse`).

- **golemd has no config file.** Configuration is CLI-only —
  `--host`, `--state-dir`, `--listen`, `--reconciler` (`main.rs:31–40`). There is
  no mechanism to express a retry policy at all.

- **Errors leak raw.** Every HTTP response is an axum `Json` (`http.rs`). A
  reconcile error propagates through `blocking` (`http.rs:40`) as
  `ApiError::internal` (`http.rs:109`), whose body is `format!("{e:#}")` — a raw
  `anyhow`/rusqlite string, HTTP 500 (`http.rs:118–120`). This is the bad UX the
  user hit: a `Conversion error from type Text …` rusqlite string leaked to the
  operator verbatim. `fleet apply` prints `response.text` unmodified on any
  non-200 (`cli.py:243–246`). A storage/deserialize failure during WAL recovery
  (`from_bytes` in `foreman.rs:88`; the `PlanRoom` reads behind `wal_steps`,
  `latest_attempt`, etc.) surfaces the same way.

- **The ordering contract is implicit.** `reconcile::plan` (`reconcile.rs:23`)
  emits installs/replaces in **scroll source order** first, then removes last
  (`reconcile.rs:27–51`; test `installs_precede_removes_and_follow_desired_order`,
  `reconcile.rs:121`). This is the enact order, author-controlled, with no
  dependency graph — but it is documented only as a code comment, not stated as a
  contract Emet authors can rely on.

Four forces drive this ADR:

- **One flaky glyph should not veto the rest.** With fail-fast, a single package
  whose mirror is briefly down blocks every later glyph in the scroll, even
  independent ones. The operator wants "apply as much as you can, tell me exactly
  what didn't take, and try the stragglers again."

- **Failure and retry must be scoped to a unit, not the whole host.** A host runs
  many distinct things — the ADR 0031 motivating fleet is one box running several
  Fishnet clients plus other units. One unit exhausting its retries, or rolling
  back, must not undo or block a sibling unit. ADR 0031 makes `Scroll` a recursive
  tree whose **leaf unit** (a scroll holding glyphs) is a failure-isolation
  boundary; this ADR's best-effort loop, retry budget, and `on_exhaust` therefore
  operate **per leaf unit**, not over the flat host scroll.

- **The retry policy is an operational knob, not a compile-time constant.**
  Different fleets want different patience: a fast local VM wants tight retries; a
  flaky-network host wants backoff with a wall-time ceiling. `5 × 200ms` fixed is
  neither.

- **Error surfacing is the product.** When best-effort lets a reconcile fail
  *several* goals at once, "one error string" is structurally wrong. The client
  needs a per-glyph report — what settled, what failed, why, how many tries, and
  whether golem rolled it back — not a 500 body carrying a rusqlite internal.

Framed as unidirectional data flow (`lw:unidirectional-data-flow`): desired state
flows in → the pure diff folds it into an ordered plan → the plan is enacted with
durable, bracketed side effects (ADR 0020) → recorded state and a **report** are
the fold's outputs. Today the enact stage short-circuits on the first failure and
the output channel collapses a structured result into a string. This ADR restores
both: enact every op, and return the structured fold.

## Decision

**Best-effort reconcile with a configurable, per-unit retry spine and a
structured, tree-shaped failure report — all built on the existing WAL,
preserving ADR 0020.** Enact walks the leaf units of the host scroll (ADR 0031
§2) in source order; each unit is best-effort enacted, retried, and — on
exhaustion — settled by its own `on_exhaust`, with siblings untouched.

### 1. Best-effort enact: try every op in a unit, retry the failed subset

Replace fail-fast with a **unit-level retry loop** whose unit of work is a
*round*: one pass that attempts every op in **this leaf unit** still owing work.
The enact spine walks the host scroll's leaf units in source order; the round loop
below runs *within* one unit, and one unit's outcome never rolls back or blocks a
sibling unit (ADR 0031 §2).

- **Round 1** runs every planned op exactly as today, each op still bracketed by
  the WAL (`Intended` before the reconciler, `Done`/`Failed` after —
  `enact_apply`/`enact_reverse`, `foreman.rs:197`, `250`). The change is that a
  `Failed` op **no longer aborts the loop**: `enact` records the `Failed` WAL row
  and *continues* to the next op. `Fatal` failures are terminal for that op — they
  are recorded `Failed` and never retried. `Retryable` failures are candidates for
  the next round.

- **Between rounds**, the loop computes the **remaining set** — the ops whose
  latest result was `Failed` *and* whose failure class was `Retryable` — from the
  round's **in-memory** classifications (`remaining_ops` over the per-op
  `StepClass` vec, `foreman.rs`), not from a WAL re-read. The failure *class*
  (retryable vs fatal) is a live-round concern only: the WAL never stores it —
  every op is still bracketed `Intended`→`Done`/`Failed` exactly as before, and
  the `Failed` row records *that* the op failed, not its retryability. This keeps
  ADR 0020 intact: the WAL bracketing invariant is unchanged, and crash recovery
  still folds the WAL by the same ADR 0020 §3 path — which needs only the
  bracket, never the class, because a re-driven `Failed` op is re-classified when
  it runs again. Each retried op appends a fresh `Intended`→`Done`/`Failed`
  bracket (a new WAL step transition; append-only, no UPDATE), so a step's round
  history is legible in the log. (The plan flagged this interpretation — the class
  lives with the live round loop, not on the wire; the WAL carries the durable
  bracket, the in-memory `StepClass` carries the transient retry decision.)

- **The loop waits per this unit's resolved retry pace (§3) between rounds** and
  stops when either the remaining set is empty (this unit fully succeeded) or the
  unit's total limit trips (§3). This
  **subsumes the per-op `attempt`/`attempt_reverse` spines**: there is now **one**
  retry spine, at the unit level, not two at the op level. `attempt`/
  `attempt_reverse` (`foreman.rs:456–488`) are deleted; `enact_apply`/
  `enact_reverse` call `self.reconciler.apply`/`reverse` **once** per round and
  return the classified result to the round loop. This also removes the current
  duplication and lets a slow, independent glyph retry *concurrently in wall-clock
  terms* with others in the same round rather than serially draining its own five
  tries before the next glyph is even attempted.

Ordering is unchanged: leaf units enact in source order (ADR 0031 §2), and within
a unit a round runs the unit's glyphs in source order, installs/replaces then
removes (§5). A `Noop` still writes no WAL step. An in-place `Replace` (ADR 0020
§4) stays a single `apply`; a reverse-then-apply `Replace`/`Remove` keeps its two
brackets, and either bracket failing puts *that op* in its unit's remaining set.
A vanished unit's removes run as their own unit under the surviving parent
scroll's resolved policy, with a synthetic `<removes>` terminal path segment
keeping that group's `unit_path` disjoint from any present unit's — otherwise a
removes-group rollback could reverse a present unit's applied steps (ADR 0031 §4).

### 2. Immediate failure logging

Every failure is logged **at the moment its `Failed` WAL row is written** — never
deferred until retries drain. The log points, all in `enact_apply`/`enact_reverse`
at the `Failed`-arm (`foreman.rs:230`, `281`):

- **Retryable, will retry** → `warn!(glyph_key, round = n, class = "retryable", reason = %msg, "enact failed; will retry")`.
- **Retryable, limit reached** → `error!(glyph_key, round = n, class = "retries-exhausted", reason = %msg, "enact failed; giving up")` — emitted when the round loop decides this op is done owing to the total limit.
- **Fatal** → `error!(glyph_key, round = n, class = "fatal", reason = %msg, "enact failed; not retryable")`.
- **Rollback step failed** (during `rollback_attempt`, `foreman.rs:439`) keeps its
  existing `warn!`, now enriched with `glyph_key` + `phase = "reverse"`.

Only glyph *keys* and failure reasons are logged — never file contents or secrets
(the ADR 0020 logging discipline). This satisfies "failures are visible in the
journal in real time," which `fleet logs` already tails.

### 3. Configurable retry policy: fleet default in a config file, overridable per scroll

golemd gains a **config file** (new `apps/golemd/src/config.rs` module), TOML,
default path resolved from a new optional `--config` flag on `main.rs`'s `Cli`
(`main.rs:31`). The `[retry]` block is the **fleet-wide default** — the fallback
every leaf unit inherits when neither it nor any ancestor scroll sets a field. It
is **not** the final word: ADR 0031 §3's per-scroll `policy` cascade overrides it,
nearest scope winning — `golemd.toml [retry]` → ancestor branch scroll `policy` →
leaf unit `policy`. A unit that sets no policy, under ancestors that set none,
runs on the `golemd.toml` defaults exactly as before.

The file is **optional**: absent, every field falls back to a built-in default, so
today's CLI-only invocation keeps working. A field present in the file overrides
its default; `--config` names a non-default path.

```toml
# golemd.toml — all fields optional; shown values are the built-in defaults.
[retry]
base_delay_ms     = 200     # delay before the first retry round
backoff_multiplier = 2.0    # each round multiplies the prior delay
max_delay_ms      = 30000   # ceiling on the per-round delay (backoff saturates here)
jitter_fraction   = 0.2     # ± this fraction of the delay, uniform, to de-synchronize retries
max_attempts      = 5       # hard cap on rounds per op (round 1 + up to 4 retries)
max_elapsed_ms    = 120000  # wall-time budget for the whole reconcile's retrying
on_exhaust        = "rollback"  # "rollback" | "keep" — behavior when a limit trips (§4)
```

Semantics:

- The per-round delay is `min(max_delay_ms, base_delay_ms × backoff_multiplier^(round-1))`,
  then perturbed by ± `jitter_fraction` (uniform). Jitter matters across a fleet:
  without it, N hosts that failed the same upstream retry in lockstep, hammering it.
- **Both limits apply, whichever trips first.** `max_attempts` bounds rounds per
  op; `max_elapsed_ms` bounds the reconcile's total retrying wall-time (measured
  from the attempt opening). A long backoff can exhaust the time budget before the
  attempt count — either ends retrying. This is deliberately belt-and-suspenders:
  attempt count bounds a fast-failing loop, wall-time bounds a slow-backoff one.
- `on_exhaust` selects behavior after a limit trips with ops still failing (§4).

`config.rs` is a plain deserialize (`serde` + `toml`) into a `RetryConfig` struct
with `Default`; `Foreman::with_retry` is replaced by `Foreman::with_retry_config`
taking the parsed struct. Per unit, the effective `RetryConfig` is this default
merged with the scroll `policy` cascade (ADR 0031 §3), each set field overriding
the wider scope. The config *file* is golemd's private operational surface — it is
**not** part of the `scroll-format` wire contract; the per-scroll `policy` that
overrides it, by contrast, *is* carried on the wire (ADR 0031 §5), because a unit's
patience is part of how its author declared it.

### 4. Behavior after exhaustion — `on_exhaust`, scoped to the failing unit

When a **unit's** rounds end with ops still failing, its resolved `on_exhaust`
(§3, ADR 0031 §3) chooses between two honest policies — applied to **that unit's
subtree only**, never to a sibling unit. This is the **one sub-decision with real
tension against ADR 0020's reversibility guarantee**:

- **`rollback`** (default) — undo the applied steps **of this unit this attempt**
  via the existing `rollback_attempt` (`foreman.rs:423`), scoped to the unit's
  `unit_path`, and mark this unit's ops rolled back. The unit returns to its last
  committed applied set; sibling units are untouched and their outcomes stand. The
  report (§5) lists the unit's failures; nothing partial is left behind **for this
  unit**. This is the ADR 0020 all-or-nothing spine, unchanged, now reached
  per-unit after best-effort has tried everything rather than at the first error
  of the whole host.

- **`keep`** — leave this unit's successfully-applied steps in place, do **not**
  roll them back, and settle them into the committed applied set. The report marks
  which of the unit's glyphs failed and that they were **not** rolled back. This is
  the literal "reconcile as much as you can and keep it" mode, per unit.

Sibling isolation is the load-bearing change from the pre-revision ADR: one unit's
`rollback` no longer undoes another unit's applied glyphs (ADR 0031 §2). The
attempt as a whole settles once every unit has settled; a unit that rolled back
and a unit that committed coexist in the same attempt, each recorded under its own
`unit_path`.

**The default is `rollback`** (settled with Dr. Dub, ADR 0031 §3). ADR 0015/0020's
load-bearing property is that golem can always reverse exactly what it did, and
that a unit settles atomically — the unit's glyphs all take, or the unit returns
to its prior committed state. `rollback` preserves that invariant as the *default*;
an author gets predictable, all-or-nothing semantics per unit unless they
explicitly set that unit's `policy` to `keep`. `keep` is the escape hatch for a
unit that prefers forward progress over atomicity (e.g. a large package set where
getting 90% installed beats 0%), and the author who chooses it is knowingly trading
the atomic-unit guarantee for availability. The tension: `keep` weakens "a unit
settles atomically" to "a unit settles as much as it could," and a subsequent
reconcile then plans against a *partially* applied set for that unit — which is
well-defined (the WAL fold is always the truth) but is no longer the clean atomic
boundary ADR 0020 assumes. That is why `keep` is opt-in per unit and `rollback` is
the default.

Either way the WAL bracketing is untouched: every op is `Intended`→`Done`/`Failed`
regardless of `on_exhaust`, and `rollback` uses the same resumable, crash-safe
`rollback_attempt` as recovery.

### 5. Structured, tree-shaped failure report (the API becomes a report, not one error)

golemd returns a **structured `ReconcileReport`** from the write path, even on
partial failure — replacing the bare `Revision` return of `apply_manifest`
(`foreman.rs:87`) and the raw-string error path of `http.rs`. The report is
**tree-shaped**, mirroring the host scroll (ADR 0031 §6): each leaf unit reports
its own outcome, so `fleet apply`/`fleet status` render "what's on this box"
grouped by unit, and a rolled-back unit sits beside a settled sibling.

```
ReconcileReport {
    revision: Revision,              // what settled (the projected Reconcile revision, ADR 0020 addendum)
    outcome:  "settled" | "partial" | "rolled_back",  // rolled up across units
    units:    Vec<UnitReport>,       // one per leaf unit, in source order
}

UnitReport {
    unit_path: Vec<String>,          // root→leaf scroll names (ADR 0031 §4)
    outcome:   "settled" | "partial" | "rolled_back",  // this unit's outcome
    failures:  Vec<GlyphFailure>,    // this unit's failed glyphs
}

GlyphFailure {
    glyph_key: String,               // Glyph::key() — the resource identity
    unit_path: Vec<String>,          // the leaf unit this glyph belonged to
    phase:     "enact" | "reverse" | "recovery",
    class:     "fatal" | "retries-exhausted",
    attempts:  u32,                  // rounds this op ran before giving up
    message:   String,               // the reconciler's reason (no secrets)
    rolled_back: bool,               // true if this unit's on_exhaust=rollback undid it
}
```

- A **unit's** `outcome` is `settled` (no failures), `rolled_back` (its
  `on_exhaust = rollback` tripped; its `failures` each `rolled_back = true`), or
  `partial` (its `on_exhaust = keep`; failures with `rolled_back = false`).
- The **top-level** `outcome` rolls up: `settled` iff every unit settled;
  otherwise `partial` if any unit kept a partial set, else `rolled_back`. `revision`
  reflects the committed applied set across all units — every unit that settled or
  kept, minus every unit that rolled back.

**HTTP shape:** `apply_manifest` returns **HTTP 200 with the `ReconcileReport`
body in all three cases** (`http.rs:63`). Failure is carried *in-band* in
`outcome` + `failures`, not in the status code — a partial or rolled-back
reconcile is a *successful RPC that reports goal failures*, not a transport error.
This is the decisive fix for the leaked-string UX: the client always parses a
typed body. (The alternative — a documented non-2xx carrying the *same*
`ReconcileReport` — was considered and rejected for the reconcile path because it
conflates "the daemon couldn't answer" with "the daemon answered that some goals
failed"; see Alternatives. Genuine transport/daemon errors keep their non-2xx.)

**Wrap storage/deserialize failures in a typed, actionable error.** WAL
recovery/read failures (the `PlanRoom` reads in `foreman.rs`/`planroom.rs`,
`from_bytes` at `foreman.rs:88`) are today raw rusqlite/postcard strings. Introduce
a typed `ForemanError::WalUnreadable { detail }` (and `ManifestUndecodable`) that
`http.rs` maps to a **documented non-2xx (HTTP 500) with a structured JSON body**
carrying a stable `kind` and an actionable message — e.g. *"golemd couldn't read
its write-ahead log; it may be from an incompatible golemd version. Run `fleet
reset` on this host to start from a clean state."* — instead of leaking the
`Conversion error from type Text …` internal. `ApiError` (`http.rs:103`) gains a
structured JSON body (`{ kind, message }`) rather than a bare string
(`http.rs:118`).

**`fleet apply` renders the report by unit** (`cli.py:apply`, `cli.py:224`). On a
200 report it reuses the compact summary renderer (`_render_revision`,
`cli.py:206`) for the settled revision, then prints **each `UnitReport` under its
name-path**, colored by that unit's `outcome` (green `settled`, yellow `partial`,
red `rolled_back`), with a **failures block** beneath: one red line per
`GlyphFailure` — `✗ <glyph_desc>  <class> after <attempts> tries — <message>`
(reusing `_glyph_desc`, `cli.py:169`). A header line colors the whole apply by the
rolled-up top-level `outcome`. On the typed non-2xx it prints the actionable
`message`, not `response.text`.

### 6. Document the ordering contract

State it plainly, as an author-facing contract (a reference note under
`apps/emet/` docs / `apps/emet/CLAUDE.md`, cross-linked from this ADR):

> **Apply order is source order — units first, then glyphs within a unit.** golemd
> enacts a host's leaf units in the order they appear in the Emet source (ADR
> 0031 §2), and within a unit enacts its glyphs in source order: installs and
> replaces first (in source order), removes last (`reconcile::plan`,
> `reconcile.rs:23`). Ordering is **author-controlled** — if unit B must come after
> unit A, or glyph B after glyph A, write B after A. There is **no dependency DAG**
> and no automatic reordering, across units or within one; author order is the
> whole contract.

A dependency graph is **rejected** (Alternatives; and ADR 0031 rejects a
cross-unit DAG for the same reason) — author order suffices and keeps the model
closed. One refinement is **recommended and noted**: `plan` should emit
removes in **reverse-of-apply order** (reverse source order) so teardown unwinds in
the opposite order to setup — the natural safe order for dependent resources (tear
down the thing that depends before the thing depended-on). This is a small,
low-risk change to the removes pass (`reconcile.rs:44–51`) and is the only ordering
adjustment this ADR proposes; the install/replace order is unchanged.

### Preserving ADR 0020

Every mechanism this ADR adds is layered *on* the WAL, not around it:

- **Bracketing invariant intact.** Every op — first try or retry, apply or reverse
  — writes `Intended` before the reconciler and `Done`/`Failed` after. Best-effort
  changes *control flow after a `Failed` row* (continue vs. abort), never the
  bracketing.
- **Recovery intact and reused.** The live round loop's "remaining set" is an
  in-memory classification (§1), but recovery does not depend on it: a crash
  mid-retry recovers by exactly the existing
  `recover_locked`/`redrive_intended`/`rollback_attempt` path, folding the WAL by
  its brackets alone (ADR 0020 §3), which never stored the retry class. A `Failed`
  step is safe to re-drive (reconcilers observe host state first), and re-driving
  re-classifies it — so the class living only in memory costs recovery nothing.
- **Reversibility as an option, not a loss.** `on_exhaust = rollback` *is* ADR
  0020's atomic undo, unchanged — now scoped to a unit's `unit_path`. `keep` is a
  deliberate, opt-in relaxation of the atomic-*unit* boundary, documented as such
  (§4) — the reversibility *mechanism* is preserved; the author chooses whether to
  invoke it per unit.
- **Rollback scopes to a unit via `unit_path`.** `rollback_attempt`
  (`foreman.rs:423`) reverses this attempt's still-applied steps LIFO; scoping it
  to a `unit_path` reverses only that unit's steps, so a sibling unit's steps are
  never in the set. Recovery of a *whole* interrupted attempt still reverses every
  unit's steps (a crash is not a per-unit `on_exhaust` decision) — the `unit_path`
  narrows only the deliberate per-unit exhaustion rollback, not crash recovery.

### Implementation surface

- `apps/golemd/src/foreman.rs` — walk the host scroll's leaf units (ADR 0031 §2)
  in source order; per unit, a best-effort round loop replacing the fail-fast
  `enact` (`:158`); delete `attempt`/`attempt_reverse` (`:456`, `:473`); classify
  and collect failures per unit; `on_exhaust` branch at per-unit settle/rollback
  (`:135`), `rollback_attempt` scoped to the unit's `unit_path`; immediate logging
  at the `Failed` arms; `apply_manifest` returns a tree-shaped `ReconcileReport`.
- `apps/golemd/src/journal.rs`/`planroom.rs` — `WalStep` gains `unit_path` and the
  `wal_step` table a `unit_path` column (ADR 0031 §6); the effective per-unit
  `RetryConfig` is the file default merged with the scroll `policy` cascade (ADR
  0031 §3).
- `apps/golemd/src/config.rs` — **new** module: `RetryConfig` (serde/toml,
  `Default`), file loader — the fleet-wide default the scroll `policy` overrides.
- `apps/golemd/src/main.rs` — `--config` flag (`:31`); load config; pass
  `RetryConfig` into `Foreman::with_retry_config`.
- `apps/golemd/src/reconciler.rs` — `EnactError { Retryable, Fatal }` (`:16`)
  unchanged in shape; its classification now feeds the round loop's remaining-set
  decision rather than a per-op bail.
- `apps/golemd/src/http.rs` — `ReconcileReport` serialize type; 200-with-report
  for reconcile; structured `ApiError` body (`{ kind, message }`) for typed
  `ForemanError` (`:103`, `:118`).
- `apps/golemd/src/foreman.rs`/`planroom.rs` — typed `ForemanError::WalUnreadable`
  / `ManifestUndecodable` wrapping rusqlite/postcard failures.
- `apps/fleet/cli.py` — `apply` renders the report's failures block and typed
  errors (`:224`), reusing `_render_revision`/`_glyph_desc` (`:206`, `:169`).
- Docs — the ordering-contract note (`apps/emet/CLAUDE.md` / an Emet reference
  page); `QUICKSTART.md` mention of `golemd.toml`/`--config`.

## Alternatives considered

1. **Keep fail-fast (do nothing).** Rejected: it is the defect — one flaky glyph
   vetoes every later glyph, and the operator learns about only the first failure.
   Best-effort with a bounded retry is strictly more informative and no less safe
   (with `on_exhaust = rollback` the settle/rollback outcome is identical to
   today's, only reached after trying everything).

2. **Two retry spines (keep per-op `attempt`, wrap a best-effort loop around it).**
   Rejected: nesting a per-op `5×` loop inside a unit-level round loop multiplies
   attempts confusingly (5 × rounds), serializes each glyph's retries before the
   next glyph is even tried, and duplicates the delay/limit logic. One spine at the
   unit level is simpler and gives fairer cross-glyph progress within a unit.

3. **A dependency DAG for glyph ordering.** Rejected. Author order in the scroll
   already expresses ordering, and the four-glyph model carries no edge kind to
   represent a dependency (root `CLAUDE.md`; ADR 0020 §5 rejected an Emet edge for
   the same reason). A DAG adds a scheduler, cycle detection, and a new manifest
   concept to solve a problem author order already solves. Removes-in-reverse is the
   one honest ordering refinement (§6) and needs no graph.

4. **Non-2xx for a partial/rolled-back reconcile.** Rejected for the reconcile
   path: a reconcile that ran and reported per-glyph failures is a *successful RPC
   with a structured result*, not a transport failure; making it non-2xx conflates
   "daemon unreachable / couldn't read its WAL" (genuinely non-2xx, §5) with
   "daemon reports some goals failed." 200-with-report keeps the client on one
   typed parse path and reserves non-2xx for real daemon/transport errors — which
   now also carry a structured `{ kind, message }` body, never a raw string.

5. **Leave errors as strings, just prettify in `fleet`.** Rejected: string-parsing
   a rusqlite internal in the CLI is brittle and version-coupled. The fix belongs
   at the source — golemd returns a typed, structured result; the CLI renders it.

## Consequences

- **A flaky glyph no longer vetoes its unit, and a failing unit no longer vetoes
  its siblings.** Every op in a unit is attempted; failures are collected and the
  retryable subset retried with backoff+jitter under a dual limit; each unit
  settles independently (ADR 0031 §2). The operator sees *all* failures, per glyph,
  grouped by unit, in one report.
- **Failures are visible in real time.** Each is logged the moment its `Failed` row
  is written (§2), tailed by `fleet logs`, in addition to the final report.
- **Retry is an operational knob with a per-unit override.** `golemd.toml [retry]`
  makes patience, backoff, jitter, and limits per-fleet defaults instead of two
  hardcoded constants; the file is optional and defaults reproduce sane behavior. A
  scroll `policy` (ADR 0031 §3) overrides it per unit or per subtree.
- **The API stops leaking internals.** `apply` returns a typed `ReconcileReport`
  (200 in-band failures) and typed `{ kind, message }` bodies for daemon errors;
  the `Conversion error from type Text …` class of leaked rusqlite string is gone,
  replaced by an actionable "read failed — `fleet reset`" message.
- **`fleet apply` shows a real report** — per-unit blocks under their name-paths,
  each a compact per-glyph failures list colored by that unit's outcome, reusing
  the summary renderer.
- **The ordering contract is written down** — source order, units then glyphs,
  author-controlled, no DAG (across or within units); removes recommended in
  reverse order for safe teardown.
- **New tension introduced by `keep`.** With a unit's `on_exhaust = keep`, that
  unit can settle partially, so a subsequent reconcile plans against its
  partially-applied set. This is well-defined (the WAL fold is always the truth)
  but relaxes the atomic-*unit* boundary — which is why `keep` is opt-in per unit
  and `rollback` is the default. Authors choosing `keep` accept that trade
  explicitly.
- **More WAL writes under heavy retrying, now carrying a `unit_path`.** Each retry
  round appends a fresh `Intended`→terminal bracket per still-failing op, each row
  tagged with its unit. Bounded by `max_attempts` × failing-op-count, cheap next to
  the host side effects, and keeps each round's history legible in the log (ADR
  0020's append-only discipline).
- **What this forecloses:** the write path no longer returns a bare `Revision` —
  callers must handle a tree-shaped `ReconcileReport` (the CLI and any future
  client). The per-op retry spine is gone, so any future need for *per-op* (not
  per-round) retry policy would reopen §1. And "the applied set is always atomic"
  now holds per unit under the default `rollback`, not per host, by design (ADR
  0031).
- **Cross-references:** refines ADR 0014 (the reconcile loop gains a per-unit
  best-effort round spine and a structured report surface; the pure diff and
  `Reconciler` port are unchanged), ADR 0015 (the `Inverse`/reversibility model is
  preserved; `on_exhaust = rollback` is its atomic undo), ADR 0020 (the WAL
  bracketing, recovery, and `rollback_attempt` are reused; best-effort changes only
  post-`Failed` control flow, and `on_exhaust` chooses whether to invoke the atomic
  rollback — now scoped by `unit_path`), ADR 0022 (the same "carry a *list* of
  failures through the surface instead of `remove(0)`-ing to one" move, now on the
  reconcile path rather than the parse path), and — the load-bearing partner —
  [ADR 0031](0031-recursive-scroll-grouping-and-failure-isolation.md) (the leaf
  unit is the scope of everything here; the report is tree-shaped; the WAL carries
  a `unit_path`; the config default is overridden by the scroll `policy` cascade).
  The four-glyph contract and the manifest are otherwise unchanged — `golemd.toml`
  and the report shape are golemd's private operational and API surfaces; the
  scroll `policy` that overrides the config *is* on the wire (ADR 0031 §5).
