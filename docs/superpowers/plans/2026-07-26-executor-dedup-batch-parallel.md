# Within-Host Executor: Dedup, Apt Batching, Bounded Parallelism Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Optimize golemd's enact spine (ADR 0034) — dedup a `(key, cid)` declared by several units so it enacts once, batch all apt installs into one `apt-get install` with a per-glyph fallback, and run units concurrently on a bounded worker pool with per-kind locks — without changing the manifest, the four glyphs, the report shape, or the no-DAG authoring contract.

**Architecture:** Three internal changes to `foreman.rs`'s `run_reconcile`, in the order they run: **(1) dedup** — a first pass finds each distinct `(key, cid)`'s first-declaring unit; the enacting unit runs the real reconciler, the other units append a credited `Done`/`changed=false`/`Inverse::Nothing` bracket directly (identical to today's second-`apply_apt` re-observation), preserving unit-scoped rollback exactly; **(2) apt batch** — a new `Reconciler::prepare(&ops)` hook (ADR 0030-shaped, no-op default) runs once before the unit loop, collapsing all apt `Install` package names into one `apt-get install pkg…` and falling back to per-glyph on batch failure; each apt op then enacts through the normal per-unit path and observes the package already installed; **(3) parallel units** — a bounded `std::thread::scope` pool (`[enact] workers`, default 4) drains the unit queue, gated by per-kind locks in the reconciler (apt/dpkg global mutex, `lineInFile` per-target-file mutex map, systemd global `daemon-reload` mutex; filesystem free). The two non-`Send` shared cells become an `AtomicU64` step-ord allocator and a `Mutex<Option<Instant>>` retry clock. Cross-unit removes stay serial, after the unit phase.

**Tech Stack:** Rust (std threads + `std::thread::scope`, `AtomicU64`, `Mutex`; `rusqlite` per-query connection mutex already present). No new crates — golemd's `Cargo.toml` is untouched. The wire crate `scroll-format` is untouched. `[enact]` is a new golemd.toml table beside `[retry]` in `config.rs`.

## Global Constraints

- **Zero comments in implementation code.** A separate documentation agent owns every comment and doc-comment; each task carries a "Doc backlog" note listing what the documenter must later explain. Do not write `//`, `///`, or `#`-prefixed prose in the implementation code you add. Test bodies may keep the minimal structural strings the framework needs (assertion messages), but no explanatory comments.
- **TDD red-green, concurrency proven deterministically.** Write the failing test, run it red, implement minimally, run it green, commit. Every concurrency claim (two units genuinely overlap; a `daemon-reload` is serialized) is proven with a **deterministic latch/barrier** in the scripted reconciler — never a `sleep` or wall-clock timing race.
- **Wire manifest format untouched.** No change to `libs/scroll-format`, postcard field/variant order, or `format_version`. `[enact] workers` is golemd's private operational surface, like `[retry]` — never on the wire, never hashed (ADR 0034 §3, ADR 0031 §5).
- **ADR 0029 / 0031 / 0033 contracts unchanged.** The per-unit best-effort round loop, the `ReconcileReport`/`UnitReport`/`GlyphLine`/`GlyphFailure` shape (`report.rs`), the WAL bracketing invariant and its order-independent recovery fold (`wal.rs`), and **unit-scoped rollback keyed on `unit_path`** (`rollback_unit`) all stand byte-for-byte. Dedup tests MUST include the shared-key rollback invariant (only the enacting unit's rollback removes the resource; a crediting unit's rollback is a no-op) and the canary-style keep-partial case.
- **`step_ord` stays attempt-unique.** `has_terminal`, `next_reversible`, `reversed_after`, and `wal::cancelled_dones` all key on `(step_ord, action, reconcile_id)` and depend on disjoint per-unit blocks. The atomic allocator must reserve non-overlapping `[base, base+len)` ranges (ADR 0034 §3, Consequences).
- **Git.** Never `git push`. Stage only the exact paths a task touched with `git add <path> [<path> …]` (never `git add -A`/`.`). Every commit message ends with the trailer:
  ```
  Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
  ```
- **Gates.** Every Rust task gates on `cargo test -p golemd`. The final workspace task gates on `cargo test --workspace`, `cargo fmt --check` clean, and **no new** `cargo clippy` warnings (`cargo clippy -p golemd --all-targets`). A final live-smoke task is **controller-run** (the operator applies the fishnet-farm example to a running scaly; the agent does not spin up a VM).

---

## File Structure

- `apps/golemd/src/foreman.rs` — **modify.** The spine of every change. `run_reconcile`: replace the `Cell<Option<Instant>>` retry clock and `&mut u64 next_ord` with a shared `AtomicU64` allocator and `Mutex<Option<Instant>>` clock (Task 1); build a dedup plan and split each unit's ops into enact-vs-credit (Task 2); call `reconciler.prepare(&ops)` before the unit loop (Task 3); run the unit loop on a bounded thread pool (Task 5). `enact_unit` loses `&mut next_ord`/`&Cell`, takes the shared allocator/clock instead. New helper `enact_credited` appends the credited bracket for a deduped glyph. New free functions `dedup_plan`, `apt_install_names`.
- `apps/golemd/src/reconciler.rs` — **modify.** Add `fn prepare(&self, _ops: &[GlyphOp]) -> EnactResult<()> { Ok(()) }` (no-op default) to the `Reconciler` trait, and forward it in the `Arc`/`Box`/`PanicCatching` impls. `PanicCatching::prepare` contains a panic like `apply`.
- `apps/golemd/src/reconcilers.rs` — **modify.** `HostReconciler` gains three shared guards: `apt: Mutex<()>`, `line_locks: Mutex<BTreeMap<String, Arc<Mutex<()>>>>`, `daemon_reload: Mutex<()>`. `prepare` gathers distinct apt `Install` names and runs one batched `apt-get install`, falling back per-glyph on failure. `apply_apt`/`reverse_apt` take the apt mutex; `apply_line_in_file` takes the per-path mutex; the two `daemon-reload` call sites (`apply_systemd`, `try_restart`) take the `daemon_reload` mutex around the reload only.
- `apps/golemd/src/config.rs` — **modify.** Add an `[enact]` table (`workers: Option<usize>`) to `FileShape`, an `EnactConfig { workers: usize }` with `Default` = 4, and fold it in `load`. `load` returns `(RetryConfig, EnactConfig)` (or a small `GolemdConfig` struct) — update the one caller in `main.rs`.
- `apps/golemd/src/main.rs` — **modify.** Thread the parsed `EnactConfig` into the foreman via a new `with_enact_config`.
- **No new files.** Every change lands in existing modules; tests live in each module's `#[cfg(test)] mod tests`.

---

## Task 1: Sync-safe plumbing (AtomicU64 step-ord, Mutex retry clock)

Pure refactor. `enact_unit`'s `next_ord: &mut u64` becomes a shared `&AtomicU64`; the `retry_clock: Cell<Option<Instant>>` becomes a `Mutex<Option<Instant>>`. No behavior changes — every existing test stays green. This makes the shared executor state `Send + Sync` so Task 5 can move units across threads (ADR 0034 §3, "the types that must change").

**Files:**
- Modify: `apps/golemd/src/foreman.rs` (`run_reconcile` ~312–353, `enact_unit` ~486–544)
- Test: `apps/golemd/src/foreman.rs` `#[cfg(test)] mod tests` (all existing tests are the regression gate; add one focused allocator test)

**Interfaces:**
- Consumes: nothing new.
- Produces:
  - `fn enact_unit(&self, reconcile_id: u64, next_ord: &std::sync::atomic::AtomicU64, ops: &[GlyphOp], prior: &[Outcome], unit_path: &[String], retry: &RetryConfig, retry_clock: &Mutex<Option<Instant>>) -> Result<UnitResult>` — the new signature Task 2 and Task 5 call.
  - The step-ord block for a unit is reserved by `next_ord.fetch_add(ops.len() as u64, Ordering::SeqCst)`, which returns the block base.
  - The retry clock is read/set under its `Mutex`: first unit to reach a retry decision sets `Some(Instant::now())`; all units read that shared start (ADR 0029 §3 budget semantics preserved).

- [ ] **Step 1: Write the failing test**

Add to `foreman.rs` tests (near `resolve_retry_uses_config_when_no_policy`):

```rust
#[test]
fn atomic_step_ord_reserves_disjoint_blocks() {
    use std::sync::atomic::{AtomicU64, Ordering};
    let next = AtomicU64::new(0);
    let a = next.fetch_add(3, Ordering::SeqCst);
    let b = next.fetch_add(2, Ordering::SeqCst);
    assert_eq!(a, 0);
    assert_eq!(b, 3);
    assert_eq!(next.load(Ordering::SeqCst), 5);
    assert!(b >= a + 3, "the second block never overlaps the first");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p golemd atomic_step_ord_reserves_disjoint_blocks`
Expected: FAIL to compile only if the import is wrong; this test itself passes against std — so instead run the FULL suite first to capture the green baseline, then treat the refactor's compile as the red/green. Run: `cargo test -p golemd`
Expected: PASS (baseline green before the refactor).

- [ ] **Step 3: Refactor `run_reconcile` to the atomic allocator and mutex clock**

In `foreman.rs`, replace the `use std::cell::Cell;` import with `use std::sync::atomic::{AtomicU64, Ordering};` (keep `use std::sync::Mutex;`). In `run_reconcile`, replace:

```rust
        let retry_clock: Cell<Option<Instant>> = Cell::new(None);
        let units = desired.scroll.leaf_units();
        let mut unit_reports = Vec::new();
        let mut next_ord: u64 = 0;
        for unit in &units {
```

with:

```rust
        let retry_clock: Mutex<Option<Instant>> = Mutex::new(None);
        let units = desired.scroll.leaf_units();
        let mut unit_reports = Vec::new();
        let next_ord = AtomicU64::new(0);
        for unit in &units {
```

and update both `enact_unit` calls (unit loop and removes loop) to pass `&next_ord` and `&retry_clock` (they already pass `&retry_clock`; change `&mut next_ord` to `&next_ord`).

- [ ] **Step 4: Refactor `enact_unit`'s signature and body**

Change the signature to take `next_ord: &AtomicU64` and `retry_clock: &Mutex<Option<Instant>>`. Replace the block reservation:

```rust
        let base_ord = *next_ord;
        *next_ord += ops.len() as u64;
```

with:

```rust
        let base_ord = next_ord.fetch_add(ops.len() as u64, Ordering::SeqCst);
```

Replace the three `retry_clock` uses in the round loop:

```rust
            if retry_clock
                .get()
                .is_some_and(|clock| clock.elapsed().as_millis() as u64 >= retry.max_elapsed_ms)
            {
                break;
            }
            if retry_clock.get().is_none() {
                retry_clock.set(Some(Instant::now()));
            }
```

with:

```rust
            {
                let clock = retry_clock.lock().unwrap();
                if clock
                    .is_some_and(|c| c.elapsed().as_millis() as u64 >= retry.max_elapsed_ms)
                {
                    break;
                }
            }
            {
                let mut clock = retry_clock.lock().unwrap();
                if clock.is_none() {
                    *clock = Some(Instant::now());
                }
            }
```

- [ ] **Step 5: Run the whole suite to verify green (behavior unchanged)**

Run: `cargo test -p golemd`
Expected: PASS — all existing tests, including `max_elapsed_bounds_the_whole_reconcile_not_each_unit` and every rollback/isolation test, unchanged. The new `atomic_step_ord_reserves_disjoint_blocks` passes.

- [ ] **Step 6: Commit**

```bash
git add apps/golemd/src/foreman.rs
git commit -m "refactor(golemd): sync-safe step-ord allocator and retry clock

AtomicU64 step-ord block reservation and Mutex<Option<Instant>> retry
clock replace the non-Send &mut u64 / Cell, leaving all behavior
identical (ADR 0034 §3). Prepares the enact spine to move units across
threads.

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** why `SeqCst` (attempt-unique `step_ord` blocks the WAL predicates depend on); why the clock moved from `Cell` to `Mutex` (shared attempt-wide budget readable from N workers); that this task is pure refactor.

---

## Task 2: Dedup a `(key, cid)` declared by several units

An identical `(key, cid)` across N units enacts **once** — under the first-declaring unit in source order. The other N−1 units record a credited bracket (`Done`, `changed = false`, `Inverse::Nothing`) directly, without calling the reconciler. This reproduces exactly today's second-`apply_apt` re-observation outcome, so unit-scoped rollback is unchanged: only the enacting unit holds the real inverse and can undo the host change; a crediting unit's rollback reverses a no-op (ADR 0034 §1).

**Files:**
- Modify: `apps/golemd/src/foreman.rs` (`run_reconcile` unit loop; new `dedup_plan` free fn; new `enact_credited` method; `enact_unit` gains a `credited: &[bool]` mask or splits ops)
- Test: `apps/golemd/src/foreman.rs` tests

**Interfaces:**
- Consumes: `enact_unit` from Task 1.
- Produces:
  - `fn dedup_plan(unit_ops: &[Vec<GlyphOp>]) -> Vec<Vec<bool>>` — parallel to `unit_ops`; `true` at `[u][i]` means unit `u`'s op `i` is a **credited** (already-enacted-by-an-earlier-unit) op. A `(key, cid)` is credited iff an earlier unit (or earlier op in the same unit) already declared the same `key` with the same content id. Only `Install`/`Replace`/`Noop` are dedup candidates; `Remove` is never in the per-unit ops here (filtered upstream). The first occurrence is `false` (it enacts).
  - `enact_unit` gains a parameter `credited: &[bool]` (one flag per op, same length as `ops`); a `true` flag routes `enact_one` to `enact_credited` instead of the real apply/reverse.
  - `fn enact_credited(&self, reconcile_id: u64, ord: u64, op: &GlyphOp, unit_path: &[String]) -> Result<StepClass>` — appends `Intended` then `Done` with `changed = Some(false)` and `Inverse::Nothing` for the op's key/action, returns `StepClass::Ok`. Never calls the reconciler.

- [ ] **Step 1: Write the failing test — a shared key enacts once, both units bracket it**

```rust
#[test]
fn a_shared_key_across_units_enacts_once_and_credits_the_rest() {
    let reconciler = ScriptedReconciler::new().ok_default();
    let foreman = foreman_with(reconciler);
    let scroll = branch_scroll(
        "host",
        vec![
            leaf_scroll("first", vec![apt("podman"), apt("only-first")]),
            leaf_scroll("second", vec![apt("podman"), apt("only-second")]),
        ],
    );
    let report = foreman.apply_scroll(scroll).unwrap();
    let applies = foreman
        .rec
        .events()
        .iter()
        .filter(|e| e.as_str() == "apply apt:podman")
        .count();
    assert_eq!(
        applies, 1,
        "the shared apt:podman glyph is enacted exactly once across the two units"
    );
    assert!(applied_keys(&foreman).contains(&"apt:podman".to_string()));
    assert!(applied_keys(&foreman).contains(&"apt:only-first".to_string()));
    assert!(applied_keys(&foreman).contains(&"apt:only-second".to_string()));
    let first = report.units.iter().find(|u| u.unit_path.last().unwrap() == "first").unwrap();
    let second = report.units.iter().find(|u| u.unit_path.last().unwrap() == "second").unwrap();
    assert!(first.glyphs.iter().any(|g| g.glyph_key == "apt:podman"));
    assert!(second.glyphs.iter().any(|g| g.glyph_key == "apt:podman"),
        "the second unit still reports its own apt:podman bracket");
}
```

Note: this requires `ScriptedReconciler` to record `apply <key>` in its `events` vec. Its `apply` currently records nothing; **add** an `events` push at the top of `ScriptedReconciler::apply` (before the fatal/retryable checks) — a one-line change in the test fake:

```rust
        self.events.lock().unwrap().push(format!("apply {}", glyph.key()));
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p golemd a_shared_key_across_units_enacts_once`
Expected: FAIL — `applies == 2` (both units enact today), and possibly a compile error until `dedup_plan`/`enact_credited` exist.

- [ ] **Step 3: Implement `dedup_plan`**

Add near `leaf_as_scroll` in `foreman.rs`:

```rust
fn dedup_plan(unit_ops: &[Vec<GlyphOp>]) -> Vec<Vec<bool>> {
    let mut seen: std::collections::BTreeSet<(String, ContentId)> = std::collections::BTreeSet::new();
    let mut credited = Vec::with_capacity(unit_ops.len());
    for ops in unit_ops {
        let mut flags = Vec::with_capacity(ops.len());
        for op in ops {
            let ident = (op.key(), enacted_cid_of(op));
            flags.push(!seen.insert(ident));
        }
        credited.push(flags);
    }
    credited
}

fn enacted_cid_of(op: &GlyphOp) -> ContentId {
    match op {
        GlyphOp::Install { cid, .. } | GlyphOp::Noop { cid, .. } | GlyphOp::Remove { cid, .. } => *cid,
        GlyphOp::Replace { new_cid, .. } => *new_cid,
    }
}
```

- [ ] **Step 4: Implement `enact_credited` and wire it into `enact_one`**

Add the method on `Foreman`:

```rust
fn enact_credited(
    &self,
    reconcile_id: u64,
    ord: u64,
    op: &GlyphOp,
    unit_path: &[String],
) -> Result<StepClass> {
    self.planroom.append_wal_step(
        reconcile_id,
        ord,
        &op.key(),
        WalAction::Apply,
        WalStepState::Intended,
        op,
        Some(&Inverse::Nothing),
        None,
        unit_path,
    )?;
    self.planroom.append_wal_step(
        reconcile_id,
        ord,
        &op.key(),
        WalAction::Apply,
        WalStepState::Done,
        op,
        Some(&Inverse::Nothing),
        Some(false),
        unit_path,
    )?;
    Ok(StepClass::Ok)
}
```

Thread a `credited: &[bool]` slice into `enact_unit`, and in `enact_unit`'s first-round loop and retry loop route through it. In the opening round:

```rust
        for (offset, op) in ops.iter().enumerate() {
            let class = if credited[offset] {
                self.enact_credited(reconcile_id, base_ord + offset as u64, op, unit_path)?
            } else {
                self.enact_one(reconcile_id, base_ord + offset as u64, op, prior, unit_path, 1)?
            };
            classes.push(class);
        }
```

A credited op returns `StepClass::Ok` and so never enters `remaining_ops` (the retry loop is untouched; credited ops never retry). Update both `enact_unit` call sites in `run_reconcile`: build the per-unit ops vectors first, call `dedup_plan` over them, then pass each unit's ops and its `credited` flags. The removes loop passes an all-`false` mask (`vec![false; group.ops.len()]`) — removes are never deduped.

Concretely, restructure the unit loop:

```rust
        let unit_ops: Vec<Vec<GlyphOp>> = units
            .iter()
            .map(|unit| {
                plan(&prior, &leaf_as_scroll(unit))
                    .into_iter()
                    .filter(|o| !matches!(o, GlyphOp::Remove { .. }))
                    .collect()
            })
            .collect();
        let credited = dedup_plan(&unit_ops);
        for (idx, unit) in units.iter().enumerate() {
            let effective = resolve_retry(&self.retry, &unit.policy_chain);
            let result = self
                .enact_unit(
                    reconcile_id,
                    &next_ord,
                    &unit_ops[idx],
                    &credited[idx],
                    &prior,
                    &unit.path,
                    &effective,
                    &retry_clock,
                )
                .map_err(ForemanError::Internal)?;
            unit_reports.push(unit_report_from(result));
        }
```

- [ ] **Step 5: Run the shared-key test to verify green**

Run: `cargo test -p golemd a_shared_key_across_units_enacts_once`
Expected: PASS.

- [ ] **Step 6: Write the shared-key rollback-invariant test (the load-bearing case)**

```rust
#[test]
fn only_the_enacting_unit_rollback_removes_a_shared_glyph() {
    let reconciler = ScriptedReconciler::new().fatal_on("apt:bad").ok_default();
    let foreman = foreman_with(reconciler);
    let scroll = branch_scroll(
        "host",
        vec![
            leaf_scroll("second", vec![apt("shared"), apt("bad")]),
            leaf_scroll("later", vec![apt("shared")]),
        ],
    );
    let report = foreman.apply_scroll(scroll).unwrap();
    let second = report.units.iter().find(|u| u.unit_path.last().unwrap() == "second").unwrap();
    let later = report.units.iter().find(|u| u.unit_path.last().unwrap() == "later").unwrap();
    assert_eq!(second.outcome, UnitOutcome::RolledBack,
        "the enacting unit fails on apt:bad and rolls its own glyphs back, removing apt:shared");
    assert_eq!(later.outcome, UnitOutcome::Settled,
        "the crediting unit settles: its credited bracket is a no-op it need not undo");
    assert!(!applied_keys(&foreman).contains(&"apt:shared".to_string()),
        "the enacting unit's rollback removed the shared glyph exactly once");
}
```

The enacting unit is `second` (source order lists it first, so it is the first-declaring unit for `apt:shared`); its `apt:bad` fails, rolling back its own `unit_path` steps — which include the real `apt:shared` apply (with `RemoveAptPackage`). `later`'s credited bracket carries `Inverse::Nothing`, so reversing it is a no-op — and `later` did not fail anyway, so it never rolls back.

- [ ] **Step 7: Run it and the keep-partial canary variant**

Add the canary variant proving a `keep` crediting unit stays partial without touching the shared glyph:

```rust
#[test]
fn a_keep_unit_crediting_a_shared_glyph_stays_partial() {
    let reconciler = ScriptedReconciler::new().fatal_on("apt:down").ok_default();
    let foreman = foreman_with(reconciler);
    let enacting = leaf_scroll("base", vec![apt("shared")]);
    let canary = leaf_scroll_with_policy(
        "canary",
        Policy { on_exhaust: Some(OnExhaust::Keep), ..Default::default() },
        vec![apt("shared"), apt("down")],
    );
    let report = foreman
        .apply_scroll(branch_scroll("host", vec![enacting, canary]))
        .unwrap();
    let base = report.units.iter().find(|u| u.unit_path.last().unwrap() == "base").unwrap();
    let canary_u = report.units.iter().find(|u| u.unit_path.last().unwrap() == "canary").unwrap();
    assert_eq!(base.outcome, UnitOutcome::Settled);
    assert_eq!(canary_u.outcome, UnitOutcome::Partial);
    assert!(applied_keys(&foreman).contains(&"apt:shared".to_string()),
        "the shared glyph enacted by base stays applied; the canary kept its partial state");
}
```

Run: `cargo test -p golemd only_the_enacting_unit_rollback a_keep_unit_crediting`
Expected: PASS both.

- [ ] **Step 8: Run the full suite (regression on WAL fold + isolation)**

Run: `cargo test -p golemd`
Expected: PASS — the credited bracket folds into the applied set identically to the re-observation bracket it replaces (`wal::applied_outcomes` keys on `glyph_key`, latest un-reversed `Done` wins), so `reapplying_same_scroll_is_noop_but_still_journals` and every isolation test stay green.

- [ ] **Step 9: Commit**

```bash
git add apps/golemd/src/foreman.rs
git commit -m "feat(golemd): dedup a (key,cid) declared by several units

An identical (key, cid) across N units enacts once under the
first-declaring unit; the rest append a credited Done/changed=false/
Inverse::Nothing bracket without re-probing the host (ADR 0034 §1).
Reproduces today's second-apply re-observation exactly, so unit-scoped
rollback is unchanged: only the enacting unit owns the real inverse.

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** the dedup key is `(key, cid)`; the credited bracket is byte-identical to the pre-dedup re-observation outcome; the load-bearing rollback invariant (which unit runs the reconciler changes, which unit can reverse never does); divergent-cid across units is NOT deduped and remains last-wins (the surfaced wart — flagged below, warn-only detection is out of scope of this task and tracked as a follow-up).

---

## Task 3: The `prepare(&ops)` hook + apt batching + per-glyph fallback

Add a `Reconciler::prepare(&ops)` pre-pass (ADR 0030-shaped). `run_reconcile` calls it once over the whole attempt's planned ops **before** the unit loop. `HostReconciler::prepare` gathers the distinct apt `Install` package names and runs **one** `apt-get install -y pkg1 pkg2 …`; on batch failure it falls back to per-glyph `apt-get install -y <name>`. Each apt op then still enacts through the normal per-unit path, observing the package already installed (ADR 0034 §2).

**Batch-streaming attribution decision (recorded — the ADR does not pin it, so this plan decides):** the batch `apt-get install pkg1 pkg2 …` runs in `prepare`, which holds **no** per-glyph progress context (no `reconcile_id`, no `unit_path`, no single glyph key — `prepare` takes `&[GlyphOp]`, not one op). The `CommandSink` seam is built per-glyph in `enact_apply` and is unavailable here. **Decision:** the batch runs **unstreamed** (through the plain `CommandRunner::run`, not `run_streaming`) — its output is not attributed to any one glyph, because attributing a multi-package solve to one arbitrary package would be a lie and fanning identical lines to every package's ring would be noise. Per-package attribution is preserved where it is honest: each apt op's own per-unit `enact_apply` still streams its `dpkg-query`/idempotent-observation through the normal sink, tagged to that exact glyph. This keeps the streaming contract (ADR 0033 §2, "each line records against the exact glyph that produced it") truthful. A future `prepare_streaming(&ops, sink)` that emits batch lines under a synthetic host-scoped context is left as a follow-up (flagged), not built here.

**Files:**
- Modify: `apps/golemd/src/reconciler.rs` (trait default + forwards + `PanicCatching`)
- Modify: `apps/golemd/src/reconcilers.rs` (`HostReconciler::prepare`, apt mutex field)
- Modify: `apps/golemd/src/foreman.rs` (`run_reconcile` calls `prepare` before the unit loop)
- Test: `apps/golemd/src/reconcilers.rs` tests (batch + fallback); `apps/golemd/src/foreman.rs` tests (prepare-is-called ordering)

**Interfaces:**
- Consumes: `dedup_plan` (Task 2) — `prepare` receives the deduped op set (one op per distinct package).
- Produces:
  - Trait method `fn prepare(&self, _ops: &[GlyphOp]) -> EnactResult<()> { Ok(()) }` on `Reconciler`.
  - `HostReconciler::prepare` batches apt `Install` names; `apt_install_names(ops: &[GlyphOp]) -> Vec<String>` free fn in `reconcilers.rs` collects distinct `Glyph::AptPackage` names from `Install` ops in first-seen order.
  - `run_reconcile` calls `self.reconciler.prepare(&all_ops)` where `all_ops` is the flattened deduped enacting ops (credited-flag `false` ops only; a credited apt op's package is already covered by the first-declaring unit's op). Failure of `prepare` is **not** fatal to the reconcile — `prepare` returning `Err` is logged and swallowed, because the fallback already ran per-glyph and any still-failing package will fail its own per-unit `apply_apt` and be classified there.

- [ ] **Step 1: Write the failing trait/no-op test (fake reconciler prepare is a no-op)**

In `reconciler.rs` add a tiny test module entry (or extend an existing one) — but the cleanest red is in `reconcilers.rs`. Write the batch test first there:

```rust
#[test]
fn prepare_batches_all_apt_installs_into_one_invocation() {
    let rec = HostReconciler::with_runner(FakeCommandRunner::new());
    let ops = vec![
        install_op(&apt("podman")),
        install_op(&apt("htop")),
    ];
    rec.prepare(&ops).unwrap();
    let log = runner_of(&rec).log();
    let batched = log.iter().any(|c| c == "apt-get install -y podman htop");
    assert!(batched, "expected one batched install, log was {log:?}");
    let per_glyph = log.iter().filter(|c| c.as_str() == "apt-get install -y podman").count();
    assert_eq!(per_glyph, 0, "no per-glyph install when the batch succeeds");
    assert!(runner_of(&rec).is_installed("podman") && runner_of(&rec).is_installed("htop"));
}
```

Add a test helper in `reconcilers.rs` tests:

```rust
fn install_op(glyph: &Glyph) -> GlyphOp {
    GlyphOp::Install { cid: glyph_content_id(glyph), glyph: glyph.clone() }
}
```

The `FakeCommandRunner` must model a multi-package install. Extend its `("apt-get", _) if args.first() == Some(&"install")` arm to insert **every** non-flag arg after `install`/`-y`, not just the last:

```rust
                ("apt-get", _) if args.first() == Some(&"install") => {
                    for name in args.iter().skip(1).filter(|a| !a.starts_with('-')) {
                        host.installed.insert((*name).to_string());
                    }
                    Ok(CommandOutput { status: 0, stdout: String::new(), stderr: String::new() })
                }
```

This keeps the existing single-package tests green (one name after `-y`).

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p golemd prepare_batches_all_apt_installs`
Expected: FAIL to compile — `prepare` and `install_op`/`apt_install_names` don't exist.

- [ ] **Step 3: Add the trait default and forwards**

In `reconciler.rs`, add to the `Reconciler` trait (after `apply_streaming`):

```rust
    fn prepare(&self, _ops: &[crate::journal::GlyphOp]) -> EnactResult<()> {
        Ok(())
    }
```

Add forwards to the `Arc<R>`, `Box<R>`, and `PanicCatching<R>` impls. For `PanicCatching`:

```rust
    fn prepare(&self, ops: &[crate::journal::GlyphOp]) -> EnactResult<()> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.inner.prepare(ops)))
            .unwrap_or_else(|payload| Err(EnactError::Fatal(panic_message(payload))))
    }
```

For `Arc<R>` and `Box<R>`:

```rust
    fn prepare(&self, ops: &[crate::journal::GlyphOp]) -> EnactResult<()> {
        (**self).prepare(ops)
    }
```

Add `use crate::journal::GlyphOp;` to `reconciler.rs` if not already importing it, or use the fully-qualified path as shown.

- [ ] **Step 4: Implement `HostReconciler::prepare` + apt mutex field**

In `reconcilers.rs`, add the apt mutex to the struct and constructors:

```rust
pub struct HostReconciler<R: CommandRunner> {
    runner: R,
    apt: std::sync::Mutex<()>,
    line_locks: std::sync::Mutex<std::collections::BTreeMap<String, std::sync::Arc<std::sync::Mutex<()>>>>,
    daemon_reload: std::sync::Mutex<()>,
}
```

Update `system()` and `with_runner` to initialize all three (`apt: Mutex::new(())`, `line_locks: Mutex::new(BTreeMap::new())`, `daemon_reload: Mutex::new(())`). (`line_locks`/`daemon_reload` are unused until Task 5 — add them now so the struct is stable and Task 5 touches no constructor; a `#[allow(dead_code)]` is NOT needed because `prepare` in this task and Task 5 both reference them, but if clippy flags them add `let _ = &self.line_locks;` — prefer just landing Task 5's uses; acceptable to introduce them in Task 5 instead if a subagent prefers a tighter diff.)

Implement `prepare` and the name-gathering helper:

```rust
impl<R: CommandRunner> HostReconciler<R> {
    fn batch_install(&self, names: &[String]) -> EnactResult<()> {
        let _guard = self.apt.lock().unwrap_or_else(|p| p.into_inner());
        let mut args: Vec<&str> = vec!["install", "-y"];
        for n in names {
            args.push(n.as_str());
        }
        let installed = self.runner.run("apt-get", &args)?;
        if installed.succeeded() {
            return Ok(());
        }
        for n in names {
            let one = self.runner.run("apt-get", &["install", "-y", n])?;
            if !one.succeeded() {
                return Err(EnactError::Retryable(format!(
                    "apt-get install {n}: {}",
                    one.stderr
                )));
            }
        }
        Ok(())
    }
}

fn apt_install_names(ops: &[GlyphOp]) -> Vec<String> {
    let mut names = Vec::new();
    for op in ops {
        if let GlyphOp::Install { glyph: Glyph::AptPackage { name }, .. } = op {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
    }
    names
}
```

Add to the `impl Reconciler for HostReconciler`:

```rust
    fn prepare(&self, ops: &[GlyphOp]) -> EnactResult<()> {
        let names = apt_install_names(ops);
        if names.is_empty() {
            return Ok(());
        }
        self.batch_install(&names)
    }
```

Note the batch does **not** run `apt-get update` here — the ADR 0030 index refresh is a separate hook that will slot into this same `prepare` when it lands; today's per-glyph `apply_apt` still runs its own `update` guarded by presence, and after the batch installs a package `apply_apt` observes it present and skips both `update` and `install`. (The `FakeCommandRunner` models a bare `install` succeeding without a prior `update`, matching real apt when the batch's own resolution works; the per-glyph fallback likewise.)

- [ ] **Step 5: Run the batch test to verify green**

Run: `cargo test -p golemd prepare_batches_all_apt_installs`
Expected: PASS.

- [ ] **Step 6: Write the batch-failure → per-glyph fallback test**

Add a scripted runner whose batched (multi-package) install fails but per-package installs succeed except one bad package. Extend `reconcilers.rs` tests with a small fake:

```rust
struct FlakyBatchRunner {
    inner: FakeCommandRunner,
}
impl FlakyBatchRunner {
    fn new() -> Self { Self { inner: FakeCommandRunner::new() } }
}
impl CommandRunner for FlakyBatchRunner {
    fn run(&self, program: &str, args: &[&str]) -> EnactResult<CommandOutput> {
        if program == "apt-get" && args.first() == Some(&"install") {
            let pkgs: Vec<&str> = args.iter().skip(1).filter(|a| !a.starts_with('-')).copied().collect();
            if pkgs.len() > 1 {
                return Ok(CommandOutput { status: 100, stdout: String::new(), stderr: "batch unresolved".into() });
            }
            if pkgs == ["nope"] {
                return Ok(CommandOutput { status: 100, stdout: String::new(), stderr: "no such package nope".into() });
            }
        }
        self.inner.run(program, args)
    }
}

#[test]
fn a_failed_batch_falls_back_to_per_glyph_installs() {
    let rec = HostReconciler::with_runner(FlakyBatchRunner::new());
    let ok = rec.prepare(&[install_op(&apt("podman")), install_op(&apt("htop"))]);
    assert!(ok.is_ok(), "two good packages install per-glyph after the batch fails");

    let bad = rec.prepare(&[install_op(&apt("podman")), install_op(&apt("nope"))]);
    match bad {
        Err(EnactError::Retryable(m)) => assert!(m.contains("nope"), "the fallback fails only the bad package: {m}"),
        other => panic!("expected a Retryable naming the bad package, got {other:?}"),
    }
    assert!(rec.inner_installed("podman"), "the good sibling still installed via fallback");
}
```

Give `FlakyBatchRunner` an `inner_installed` accessor (or read through `runner_of(&rec).inner...`); simplest is a method:

```rust
impl FlakyBatchRunner {
    fn inner_installed(&self, p: &str) -> bool { self.inner.is_installed(p) }
}
```

and call it via `rec`'s runner: `runner_of(&rec).inner_installed("podman")`.

- [ ] **Step 7: Run the fallback test to verify green**

Run: `cargo test -p golemd a_failed_batch_falls_back_to_per_glyph`
Expected: PASS.

- [ ] **Step 8: Wire `prepare` into `run_reconcile` and prove ordering (prepare before units)**

In `foreman.rs` `run_reconcile`, after `let credited = dedup_plan(&unit_ops);` and before the unit loop:

```rust
        let enacting_ops: Vec<GlyphOp> = unit_ops
            .iter()
            .zip(credited.iter())
            .flat_map(|(ops, flags)| {
                ops.iter().zip(flags.iter()).filter(|(_, c)| !**c).map(|(o, _)| o.clone())
            })
            .collect();
        if let Err(e) = self.reconciler.prepare(&enacting_ops) {
            warn!(error = %format!("{e:?}"), "prepare pre-pass reported a failure; per-unit enact will classify");
        }
```

Add a foreman-level test with the `ScriptedReconciler`. Give `ScriptedReconciler` a `prepare` impl that records `"prepare N"` into its `events` (N = op count), so ordering vs the first `apply` is observable:

```rust
    fn prepare(&self, ops: &[GlyphOp]) -> EnactResult<()> {
        self.events.lock().unwrap().push(format!("prepare {}", ops.len()));
        Ok(())
    }
```

Test:

```rust
#[test]
fn prepare_runs_once_before_any_unit_enact() {
    let reconciler = ScriptedReconciler::new().ok_default();
    let foreman = foreman_with(reconciler);
    let scroll = branch_scroll(
        "host",
        vec![
            leaf_scroll("a", vec![apt("podman")]),
            leaf_scroll("b", vec![apt("htop")]),
        ],
    );
    foreman.apply_scroll(scroll).unwrap();
    let events = foreman.rec.events();
    let prepares: Vec<usize> = events.iter().enumerate()
        .filter(|(_, e)| e.starts_with("prepare "))
        .map(|(i, _)| i).collect();
    assert_eq!(prepares.len(), 1, "prepare runs exactly once");
    let first_apply = events.iter().position(|e| e.starts_with("apply ")).unwrap();
    assert!(prepares[0] < first_apply, "prepare precedes every apply");
    assert_eq!(events[prepares[0]], "prepare 2", "prepare sees both distinct enacting ops");
}
```

- [ ] **Step 9: Run it plus the full suite**

Run: `cargo test -p golemd prepare_runs_once_before_any_unit_enact && cargo test -p golemd`
Expected: PASS all. Existing single-package apt tests (`apt_updates_package_list_before_installing`, `apt_isometry_*`) stay green — they call `apply`/`apply_streaming` directly, never `prepare`, and the multi-name fake-install arm is backward-compatible.

- [ ] **Step 10: Commit**

```bash
git add apps/golemd/src/reconciler.rs apps/golemd/src/reconcilers.rs apps/golemd/src/host.rs apps/golemd/src/foreman.rs
git commit -m "feat(golemd): apt batch install via a prepare(&ops) pre-pass

A new Reconciler::prepare(&ops) hook (no-op default) runs once before the
unit loop; HostReconciler::prepare collapses all distinct apt Install
names into one apt-get install, falling back to per-glyph on batch
failure (ADR 0034 §2, ADR 0030 hook shape). The batch runs unstreamed —
a multi-package solve has no single glyph to attribute lines to; per-op
streaming stays honest in each unit's apply.

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** the `prepare(&ops)` reconcile-scoped pre-pass slot (shared with ADR 0030's future index refresh); the batch-streaming attribution decision (batch unstreamed by design; a `prepare_streaming` follow-up is flagged, not built); the correctness-preserving fallback (worst case = today's per-package behavior); why `prepare` failure is logged-and-swallowed (per-unit enact re-classifies); removes are not batched.

---

## Task 4: `[enact] workers` config

Add golemd's private `[enact]` table beside `[retry]`, parsed in `config.rs`, defaulting to 4 workers. `load` returns both configs; `main.rs` threads `EnactConfig` into the foreman.

**Files:**
- Modify: `apps/golemd/src/config.rs` (`EnactConfig`, `EnactTable`, `FileShape.enact`, `load` signature)
- Modify: `apps/golemd/src/main.rs` (destructure the new return, `.with_enact_config(enact)`)
- Modify: `apps/golemd/src/foreman.rs` (`enact: EnactConfig` field, `with_enact_config`)
- Test: `apps/golemd/src/config.rs` tests

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `pub struct EnactConfig { pub workers: usize }`, `impl Default` = `{ workers: 4 }`.
  - `pub struct GolemdConfig { pub retry: RetryConfig, pub enact: EnactConfig }` returned by `load(path) -> Result<GolemdConfig, ConfigError>`.
  - `Foreman::with_enact_config(self, cfg: EnactConfig) -> Self`; `Foreman` gains `enact: EnactConfig` (default in `new`).

- [ ] **Step 1: Write the failing config test**

```rust
#[test]
fn enact_workers_defaults_to_four_and_overrides() {
    let cfg = load(None).unwrap();
    assert_eq!(cfg.enact.workers, 4, "default worker count is 4");

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("golemd.toml");
    std::fs::write(&path, "[enact]\nworkers = 1\n").unwrap();
    let cfg = load(Some(&path)).unwrap();
    assert_eq!(cfg.enact.workers, 1, "workers = 1 is the serial fallback");
    assert_eq!(cfg.retry.max_attempts, 5, "an [enact]-only file keeps retry defaults");
}
```

Also update the two existing config tests to read `cfg.retry.max_attempts` etc. (they currently read `cfg.max_attempts` — `load` now returns `GolemdConfig`). E.g. `none_path_gives_builtin_defaults`: `assert_eq!(cfg.retry, RetryConfig::default());`.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p golemd -- config::tests`
Expected: FAIL to compile — `EnactConfig`, `cfg.enact`, `cfg.retry` don't exist.

- [ ] **Step 3: Implement `EnactConfig`/`GolemdConfig` and rework `load`**

In `config.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnactConfig {
    pub workers: usize,
}

impl Default for EnactConfig {
    fn default() -> Self {
        EnactConfig { workers: 4 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GolemdConfig {
    pub retry: RetryConfig,
    pub enact: EnactConfig,
}

#[derive(Debug, Default, Deserialize)]
struct EnactTable {
    workers: Option<usize>,
}
```

Add `enact: Option<EnactTable>` to `FileShape`. Change `load` to build a `GolemdConfig`: fold `[retry]` into `RetryConfig::default()` as today, fold `[enact]` into `EnactConfig::default()` (`if let Some(t) = shape.enact { if let Some(w) = t.workers { cfg.enact.workers = w; } }`), return `Ok(GolemdConfig { retry, enact })`. The `None` path returns `GolemdConfig { retry: RetryConfig::default(), enact: EnactConfig::default() }`.

- [ ] **Step 4: Add the foreman field and builder**

In `foreman.rs`, add `enact: EnactConfig` to `Foreman` (import `crate::config::EnactConfig`), initialize `enact: EnactConfig::default()` in `new`, and add:

```rust
    pub fn with_enact_config(mut self, cfg: EnactConfig) -> Self {
        self.enact = cfg;
        self
    }
```

- [ ] **Step 5: Update `main.rs`**

```rust
    let config = golemd::config::load(cli.config.as_deref()).with_context(|| "load golemd config")?;
    ... Foreman::new(cli.host.clone(), Box::new(planroom), reconciler)
            .with_retry_config(config.retry)
            .with_enact_config(config.enact),
```

- [ ] **Step 6: Run config tests and the full suite**

Run: `cargo test -p golemd`
Expected: PASS — new + updated config tests green; the foreman still runs serially (Task 5 uses `enact.workers`).

- [ ] **Step 7: Commit**

```bash
git add apps/golemd/src/config.rs apps/golemd/src/foreman.rs apps/golemd/src/main.rs
git commit -m "feat(golemd): add [enact] workers config (default 4)

golemd's private [enact] table beside [retry] (never on the wire); load
returns GolemdConfig { retry, enact }; the foreman stores it for the
bounded pool. workers = 1 is the serial fallback (ADR 0034 §3).

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** `[enact]` is operational config, not the wire manifest nor the per-scroll policy; `workers` default rationale (4, a function of host size — an open question in the ADR); `workers = 1` == today's serial behavior.

---

## Task 5: Bounded parallel unit execution + per-kind reconciler locks

Run the unit loop on a bounded `std::thread::scope` pool of `enact.workers` threads, gated by per-kind locks in `HostReconciler` (apt/dpkg global, `lineInFile` per-target-file, systemd `daemon-reload` global; filesystem free). Proven with deterministic latches, not sleeps: two units genuinely overlap, and a `daemon-reload` is serialized. Cross-unit removes stay serial, after the unit phase (ADR 0034 §3).

**Files:**
- Modify: `apps/golemd/src/foreman.rs` (`run_reconcile` unit loop → scoped pool; results collected by index into source order)
- Modify: `apps/golemd/src/reconcilers.rs` (take the apt mutex in `apply_apt`/`reverse_apt`; the per-path mutex in `apply_line_in_file`; the `daemon_reload` mutex around the two `daemon-reload` calls)
- Test: `apps/golemd/src/foreman.rs` tests (deterministic overlap via a latching `ScriptedReconciler`); `apps/golemd/src/reconcilers.rs` tests (daemon-reload serialization via a counting/latching runner)

**Interfaces:**
- Consumes: `enact.workers` (Task 4); `enact_unit` (Task 1/2) is `&self` and takes only `Sync` shared state (`&AtomicU64`, `&Mutex<Option<Instant>>`, `&[Outcome]`), so it is callable from N threads. `Foreman` is `Send + Sync` (all fields are: `Box<dyn PlanRoom>` and `Box<dyn Reconciler>` are `Send + Sync`, `Mutex`/`AtomicU64` are, `ProgressRegistry` is `Mutex`-guarded).
- Produces:
  - A source-order `Vec<UnitReport>` regardless of completion order (results keyed by unit index, sorted back before roll-up — `ReconcileReport::roll_up` still emits in source order per ADR 0034 Consequences).
  - The reconciler's per-kind serialization contract (locks listed above).

- [ ] **Step 1: Write the failing deterministic overlap test**

Two units must be *inside* their enact concurrently. Prove it with a barrier the scripted reconciler blocks on: unit A's apply and unit B's apply each signal arrival and wait until both have arrived. With `workers >= 2` both arrive and the barrier releases; with `workers == 1` only one ever arrives and the test would deadlock — so the test uses a **timed** rendezvous (a `Barrier` of 2 with a bounded wait) that *asserts overlap happened*, never sleeps to fake it.

Add a latching reconciler to `foreman.rs` tests:

```rust
struct OverlapReconciler {
    gate: std::sync::Arc<std::sync::Barrier>,
    both_arrived: std::sync::Arc<std::sync::atomic::AtomicBool>,
    present: Mutex<BTreeMap<String, ContentId>>,
}
impl OverlapReconciler {
    fn new() -> Self {
        Self {
            gate: std::sync::Arc::new(std::sync::Barrier::new(2)),
            both_arrived: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            present: Mutex::new(BTreeMap::new()),
        }
    }
}
impl Reconciler for OverlapReconciler {
    fn apply(&self, glyph: &Glyph, cid: ContentId) -> EnactResult<Outcome> {
        if glyph.key().starts_with("apt:latch") {
            self.gate.wait();
            self.both_arrived.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        self.present.lock().unwrap().insert(glyph.key(), cid);
        Ok(Outcome {
            op: GlyphOp::Install { cid, glyph: glyph.clone() },
            cid,
            inverse: crate::reconciler::inverse_of(glyph),
            changed: true,
        })
    }
    fn reverse(&self, _o: &Outcome) -> EnactResult<()> { Ok(()) }
}
```

`Barrier::new(2).wait()` blocks each of the two threads until both call it — so it releases **iff** both units are executing `apply` at the same time. If the pool were serial, unit A would block forever at the barrier and the test would hang; to keep a hung serial build from wedging CI, wrap the whole apply in a watchdog: spawn the apply on a thread and `recv_timeout` on a channel.

```rust
#[test]
fn two_independent_units_enact_concurrently() {
    let rec = std::sync::Arc::new(OverlapReconciler::new());
    let arrived = rec.both_arrived.clone();
    let f = Foreman::new("host".into(), Box::new(MemoryPlanRoom::new()), Box::new(rec.clone()))
        .with_retry_config(RetryConfig { max_attempts: 1, base_delay_ms: 0, ..Default::default() })
        .with_enact_config(crate::config::EnactConfig { workers: 2 });
    let scroll = {
        let mut s = branch_scroll(
            "host",
            vec![
                leaf_scroll("a", vec![apt("latch-a")]),
                leaf_scroll("b", vec![apt("latch-b")]),
            ],
        );
        s.name = "host".into();
        s
    };
    let bytes = manifest(vec![scroll]);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || { let _ = tx.send(f.apply_manifest(&bytes).map(|_| ())); });
    let done = rx.recv_timeout(std::time::Duration::from_secs(5));
    assert!(done.is_ok(), "the reconcile completed (the two units met at the barrier)");
    assert!(arrived.load(std::sync::atomic::Ordering::SeqCst),
        "both units were inside apply simultaneously — genuine overlap, not interleaving");
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p golemd two_independent_units_enact_concurrently`
Expected: FAIL — with the still-serial loop, unit A blocks at the barrier and the reconcile never finishes; `recv_timeout` returns `Err(Timeout)` and the first assertion fails. (The spawned thread is left parked; that is acceptable in a test process that exits after the assertion.)

- [ ] **Step 3: Implement the bounded scoped pool in `run_reconcile`**

Replace the sequential unit loop with a work-queue drained by `enact.workers` scoped threads. Collect `(index, UnitReport)` through a `Mutex<Vec<Option<UnitReport>>>` sized to the unit count, then flatten in index order:

```rust
        let results: Mutex<Vec<Option<UnitReport>>> = Mutex::new((0..units.len()).map(|_| None).collect());
        let queue = AtomicU64::new(0);
        let workers = self.enact.workers.max(1);
        let enact_err: Mutex<Option<anyhow::Error>> = Mutex::new(None);
        std::thread::scope(|scope| {
            for _ in 0..workers {
                scope.spawn(|| loop {
                    let idx = queue.fetch_add(1, Ordering::SeqCst) as usize;
                    if idx >= units.len() {
                        break;
                    }
                    let unit = &units[idx];
                    let effective = resolve_retry(&self.retry, &unit.policy_chain);
                    match self.enact_unit(
                        reconcile_id, &next_ord, &unit_ops[idx], &credited[idx],
                        &prior, &unit.path, &effective, &retry_clock,
                    ) {
                        Ok(result) => {
                            results.lock().unwrap()[idx] = Some(unit_report_from(result));
                        }
                        Err(e) => {
                            *enact_err.lock().unwrap() = Some(e);
                        }
                    }
                });
            }
        });
        if let Some(e) = enact_err.lock().unwrap().take() {
            return Err(ForemanError::Internal(e));
        }
        let mut unit_reports: Vec<UnitReport> =
            results.into_inner().unwrap().into_iter().flatten().collect();
```

Keep the removes loop **after** this, serial and unchanged (it still uses `&next_ord`/`&retry_clock` and an all-`false` credited mask). `unit_reports` is then extended by the removes-group reports as today. (Reconcile the two: build `unit_reports` from the pool, then push each removes-group report as the current code does.)

- [ ] **Step 4: Run the overlap test to verify green**

Run: `cargo test -p golemd two_independent_units_enact_concurrently`
Expected: PASS — with `workers = 2` both units reach the barrier, it releases, both set `both_arrived`, the reconcile completes within the watchdog.

- [ ] **Step 5: Take the per-kind locks in the reconciler**

In `reconcilers.rs`:

- `apply_apt`: wrap the whole body's command work under `let _guard = self.apt.lock().unwrap_or_else(|p| p.into_inner());` (take it right after the `apt_installed` early-return check, or before it — before is safest: the `dpkg-query` probe and install must not race a concurrent remove). Add the same guard at the top of `reverse_apt`.
- `apply_line_in_file`: acquire the per-path lock. Since `apply_line_in_file` is a free fn today, either (a) make it a method on `HostReconciler` so it can reach `self.line_locks`, or (b) pass the lock in. Prefer (a): change the dispatch in `apply_streaming` for `Glyph::LineInFile` to call `self.apply_line_in_file_locked(path, line, cid, glyph)`, which does:

```rust
    fn apply_line_in_file_locked(&self, path: &str, line: &str, cid: ContentId, glyph: &Glyph) -> EnactResult<Outcome> {
        let lock = {
            let mut map = self.line_locks.lock().unwrap_or_else(|p| p.into_inner());
            map.entry(path.to_string()).or_insert_with(|| std::sync::Arc::new(std::sync::Mutex::new(()))).clone()
        };
        let _guard = lock.lock().unwrap_or_else(|p| p.into_inner());
        apply_line_in_file(path, line, cid, glyph)
    }
```

(The free fn `apply_line_in_file` stays for the existing direct tests.)
- `apply_systemd` and `try_restart`: wrap **only** the `daemon-reload` run under `let _guard = self.daemon_reload.lock().unwrap_or_else(|p| p.into_inner()); ...; drop(_guard);` — release before `enable`/`start`/`try-restart` so per-unit lifecycle runs concurrently (ADR 0034 §3, the systemd granularity).

- [ ] **Step 6: Write the deterministic daemon-reload serialization test**

Prove two concurrent `daemon-reload`s never overlap: a runner that, on `daemon-reload`, increments an "in-flight" counter, asserts it is exactly 1, then decrements — under a real mutex the assert never trips; without the lock two reloads would race the counter to 2. Use a short spin (not a sleep-to-fake) to widen the window deterministically.

```rust
struct ReloadCountingRunner {
    inner: FakeCommandRunner,
    in_flight: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    max_seen: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}
impl ReloadCountingRunner {
    fn new() -> Self {
        Self { inner: FakeCommandRunner::new(),
               in_flight: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
               max_seen: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)) }
    }
}
impl CommandRunner for ReloadCountingRunner {
    fn run(&self, program: &str, args: &[&str]) -> EnactResult<CommandOutput> {
        if program == "systemctl" && args == ["daemon-reload"] {
            let now = self.in_flight.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            self.max_seen.fetch_max(now, std::sync::atomic::Ordering::SeqCst);
            for _ in 0..2000 { std::hint::spin_loop(); }
            self.in_flight.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        }
        self.inner.run(program, args)
    }
    fn run_streaming(&self, program: &str, args: &[&str], _s: &mut CommandSink<'_>) -> EnactResult<CommandOutput> {
        self.run(program, args)
    }
}

#[test]
fn concurrent_daemon_reloads_are_serialized() {
    let rec = std::sync::Arc::new(HostReconciler::with_runner(ReloadCountingRunner::new()));
    let max = rec.runner_max_seen();
    std::thread::scope(|s| {
        for unit in ["a", "b", "c", "d"] {
            let rec = rec.clone();
            s.spawn(move || {
                let g = systemd(unit);
                let _ = rec.apply(&g, glyph_content_id(&g));
            });
        }
    });
    assert_eq!(max.load(std::sync::atomic::Ordering::SeqCst), 1,
        "no two daemon-reloads were ever in flight at once");
}
```

Expose `runner_max_seen` via a small accessor on the test-side (e.g. store the `Arc` in a wrapper, or add `fn runner_max_seen(&self) -> Arc<AtomicUsize>` behind `#[cfg(test)]` on `HostReconciler` returning `self.runner`'s field). Simplest: construct the `ReloadCountingRunner`, clone its `max_seen` Arc **before** moving it into `with_runner`:

```rust
    let runner = ReloadCountingRunner::new();
    let max = runner.max_seen.clone();
    let rec = std::sync::Arc::new(HostReconciler::with_runner(runner));
```

(and drop the `runner_max_seen` accessor). `HostReconciler::apply` is `&self` and `Send + Sync`, so `Arc<HostReconciler<_>>` spawns across the scope.

- [ ] **Step 7: Run the daemon-reload test to verify green**

Run: `cargo test -p golemd concurrent_daemon_reloads_are_serialized`
Expected: PASS — the `daemon_reload` mutex admits one reload at a time; `max_seen == 1`.

- [ ] **Step 8: Full suite (regression: serial-equivalent behavior at workers=1, WAL fold order-independence)**

Run: `cargo test -p golemd`
Expected: PASS. Confirm specifically that the isolation/rollback tests (`a_units_rollback_undoes_only_its_own_glyphs`, `enacting_one_unit_does_not_remove_a_sibling_units_applied_glyph`, `max_elapsed_bounds_the_whole_reconcile_not_each_unit`) stay green — the default test foreman uses `foreman_with`, which does not set `[enact]`, so it runs at the default 4 workers; these tests must pass under real concurrency, proving `unit_path`-scoped rollback and the shared retry clock hold across threads. If `max_elapsed_bounds_...` becomes order-sensitive under parallelism, pin it to `workers = 1` via `.with_enact_config(EnactConfig { workers: 1 })` in that test and note the honest reason (it asserts a source-order budget-start property that only holds serially).

- [ ] **Step 9: Commit**

```bash
git add apps/golemd/src/foreman.rs apps/golemd/src/reconcilers.rs
git commit -m "feat(golemd): bounded parallel unit enact with per-kind locks

The unit loop runs on a std::thread::scope pool of [enact] workers;
HostReconciler serializes apt/dpkg (global), lineInFile (per target
file), and systemd daemon-reload (global, released before per-unit
enable/start); filesystem is unrestricted. Results are collected by
index and rolled up in source order. Cross-unit removes stay serial
after the unit phase (ADR 0034 §3). Overlap and daemon-reload
serialization proven with deterministic latches.

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** the fixed executor shape (batch → parallel units → serial removes) and that it adds no cross-unit ordering; the per-kind lock granularity and *why each* (dpkg global lock, per-file lineInFile RMW race, systemd global reprocess with concurrent per-unit lifecycle); results-by-index → source-order roll-up; the watchdog pattern in the overlap test and why a `Barrier` proves genuine overlap (not a sleep); the open question on whether the daemon-reload lock should also cover the following `enable`.

---

## Task 6: Whole-workspace gate + controller-run live smoke

Prove the whole workspace is green, formatted, and clippy-clean, then hand the operator a live-smoke script. The live run is **controller-run** — the agent does not spin up a VM.

**Files:**
- No source changes (gate task). The live-smoke steps are operator instructions.

**Interfaces:**
- Consumes: everything from Tasks 1–5.
- Produces: a green workspace and a recorded live-smoke expectation.

- [ ] **Step 1: Workspace test gate**

Run: `cargo test --workspace`
Expected: PASS — golemd, scroll-format, and every other crate green.

- [ ] **Step 2: Format check**

Run: `cargo fmt --check`
Expected: no diff (exit 0). If it reports changes, run `cargo fmt`, re-inspect the diff is only formatting, and fold it into the Task-5 commit's follow-up (`git add <changed files>` + amend is disallowed — instead a small `style(golemd): cargo fmt` commit with the trailer).

- [ ] **Step 3: Clippy — no new warnings**

Run: `cargo clippy -p golemd --all-targets`
Expected: no new warnings versus the pre-plan baseline. Common ones to preempt: `clippy::too_many_arguments` on the widened `enact_unit` — it already carries `#[allow(clippy::too_many_arguments)]` on the bracket helpers; add the same allow to `enact_unit` if clippy flags it (it now takes 8 args). A `clippy::type_complexity` on `Mutex<BTreeMap<String, Arc<Mutex<()>>>>` — if flagged, introduce a `type LineLocks = ...` alias in `reconcilers.rs` (a type alias is not a comment; allowed).

- [ ] **Step 4: Commit any fmt/clippy fixups**

```bash
git add apps/golemd/src/reconcilers.rs apps/golemd/src/foreman.rs
git commit -m "style(golemd): fmt and clippy cleanups for the executor changes

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

(Skip this step if Steps 2–3 were already clean.)

- [ ] **Step 5: Controller-run live smoke (operator-verified — the agent does NOT run this)**

Marked controller-run. The operator applies the fishnet-farm example to a running `scaly` VM and confirms the ADR 0034 wins on a real host:

```
# operator, from the golem repo root:
fleet up scaly
fleet deploy examples/fishnet-farm/farm.emet
fleet apply scaly
```

**Expected observations (record in the PR / dogfood note):**
- **One apt invocation covers both packages.** Every workload leaf pulls in `apt:podman` (via `Quadlet`) and `base` pulls in `apt:htop`; the batch `prepare` should show exactly **one** `apt-get install -y podman htop` (order may vary), not one per package or per leaf. Confirm in golemd's log / journal that a single batched install ran.
- **Shared `apt:podman` enacts once.** Across the five+ workload leaves the apt:podman glyph is applied once (the first-declaring leaf); the rest show a credited `changed=false` bracket. Confirm the TUI shows podman settling under one leaf and instant/unchanged under the others.
- **Five units overlap in the TUI.** With the default 4 workers, multiple leaves spin concurrently in the golemctl progress TUI (ADR 0033 §3) — not one-at-a-time. The canary leaf still reports `partial` (kept), every sibling green, exactly as the pre-parallel farm did — parallelism must not change any outcome, only their timing.
- **`workers = 1` reproduces serial.** Optionally, re-run with a `golemd.toml [enact] workers = 1` and confirm the same green/partial outcomes, drained one leaf at a time — the serial fallback is behavior-identical.

**Doc backlog:** the live-smoke expectations as an operator runbook entry; the observed single-apt-invocation and overlap as evidence the ADR 0034 wins landed; that `workers=1` is the serial escape hatch.

---

## Self-Review

**Spec coverage (ADR 0034 sections → tasks):**
- §1 dedup → Task 2 (`dedup_plan`, `enact_credited`, shared-key enacts-once test, shared-key rollback-invariant test, keep-partial canary test). ✓
- §1 divergent-cid wart → **flagged, narrowed.** Dedup keys on `(key, cid)` so divergent cids are not deduped (correct per ADR). The ADR asks golemd to *warn* on divergent-cid + a report note; **this plan does NOT build the warn/report detection** — it is a separate surface (progress event + report field) the ADR itself brackets as "surfaced" and leaves the emetc-error half "open." Recorded as a follow-up, not silently dropped. Called out in the final report.
- §2 apt batch + prepare hook + fallback + no-batched-removes → Task 3 (batch test, fallback test, prepare-ordering test; removes untouched by `prepare`). ✓
- §2 batch-streaming attribution → **decided in Task 3** (batch runs unstreamed; per-op streaming stays honest; `prepare_streaming` follow-up flagged). ✓
- §3 `[enact] workers` → Task 4. ✓
- §3 parallel units + per-kind locks + fixed phases → Task 5 (scoped pool; apt/lineInFile/daemon-reload locks; filesystem free; removes serial after; overlap + daemon-reload tests). ✓
- §3 non-Send types (`next_ord`→AtomicU64, retry clock→sync) → Task 1. ✓
- §3 WAL append already per-query locked / progress ring already concurrent-safe / reconciler already Send+Sync → relied on, no change (verified in Task 5 regression). ✓
- Consequences: source-order report roll-up preserved (results-by-index), WAL fold order-independent (regression suite) → Task 5. ✓

**Placeholder scan:** every code step carries complete code; commands carry expected output; the live-smoke task is explicitly operator-run with concrete expectations. No TBD/TODO left.

**Type consistency:** `enact_unit`'s widened signature (`&AtomicU64`, `credited: &[bool]`, `&Mutex<Option<Instant>>`) is introduced in Task 1/2 and used unchanged in Task 5. `prepare(&self, ops: &[GlyphOp]) -> EnactResult<()>` is identical across the trait default, forwards, `PanicCatching`, `HostReconciler`, and `ScriptedReconciler`. `GolemdConfig { retry, enact }` and `EnactConfig { workers }` are consistent from Task 4 into `main.rs` and the foreman field. `dedup_plan`/`enacted_cid_of`/`apt_install_names`/`enact_credited` names are used consistently.
