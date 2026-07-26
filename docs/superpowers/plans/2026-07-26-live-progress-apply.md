# Live-Progress Apply (async 202 + WAL projection + golemctl iocraft TUI) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `POST /manifest` asynchronous (returns `202 { reconcile_id }`), add a lock-free `GET /reconciles/<id>?after=<seq>` WAL-projection-plus-event-log endpoint, turn golemctl into a fire-then-poll live iocraft progress TUI (plain fallback for non-TTY/`--json`), and make `fleet apply` exec golemctl per host — removing the synchronous held-open transport and the ffa1414 unbounded-timeout stopgap in one lockstep change (ADR 0033).

**Architecture:** golemd's handler does only the cheap synchronous ingest gate (decode, select scroll, recover, refuse-if-unsettled) then spawns the reconcile on a blocking task and returns the attempt id at once. A new `progress` module holds a bounded per-attempt in-memory event ring (seq-keyed) that the enact spine writes to at the same call sites it already `tracing`-logs; the poll endpoint folds the attempt's durable `wal_step` rows into per-glyph states and merges the ring's `> after` slice, returning `report: null` until the attempt settles and the full unchanged `ReconcileReport` after. golemctl reads the 202, polls the projection, and renders a devenv-tui-shaped model/events/view iocraft tree (spinner per unit, log lines under the active unit) to stderr; fleet stops speaking HTTP on the apply path and execs golemctl against each host's forwarded golemd port.

**Tech Stack:** Rust (axum 0.7, tokio, rusqlite, serde) for golemd; Rust (clap, reqwest, tokio, **new `iocraft` 0.8.2**) for golemctl; Python (typer/rich, subprocess) for fleet. The wire manifest format (`scroll-format`, postcard) is untouched — this is entirely golemd's HTTP surface plus its clients.

## Global Constraints

- **Zero comments in implementation code.** A separate documentation agent owns every comment and doc-comment; each task carries a "Doc backlog" note listing what the documenter must later explain. Do not write `//`, `///`, `#`, or `"""docstring"""` prose in code you add. (Test bodies may keep the minimal structural strings the framework needs, but no explanatory comments.)
- **TDD red-green everywhere testable.** Write the failing test, run it red, implement minimally, run it green, commit. TUI *view logic* is tested at the model level (devenv-tui's `tests/tui_tests.rs` pattern: build a model, apply events, render the view to a string, assert on the string). Terminal rendering itself (the live iocraft runtime driving a real TTY) is **smoke-tested only** — stated honestly per task; there is no automated assertion on the animated terminal output.
- **Wire manifest format untouched.** No change to `libs/scroll-format`, postcard field/variant order, or `format_version`. `GET /reconciles/<id>` is golemd's HTTP JSON surface, not the manifest contract.
- **ADR 0029 report shape unchanged.** `ReconcileReport`/`UnitReport`/`GlyphLine`/`GlyphFailure` and every `serde` tag in `apps/golemd/src/report.rs` stay byte-for-byte identical. The report now arrives on the final poll's `report` field instead of the apply response — the *type* does not change.
- **The ffa1414 stopgap is removed in the fleet-delegation task.** `golemd_client.apply_manifest`, `_APPLY_TIMEOUT` (`read=None`), and `_render_apply_transport_error` are deleted; `_render_report` is retired from the apply path. There is no dual-protocol window (ADR 0013 lockstep).
- **Git.** Never `git push`. Stage only the exact paths a task touched with `git add <path> [<path> …]` (never `git add -A`/`.`). Every commit message ends with the trailer:
  ```
  Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
  ```
- **Gates.** Rust tasks gate on `cargo test -p golemd` or `cargo test -p golemctl` as noted; the workspace task gates on `cargo test --workspace`. Fleet tasks gate on the explicit test module (`python -m pytest apps/fleet/tests/test_apply_render.py`). A final live smoke task applies the fishnet-farm example to a running scaly via fleet→golemctl — **operator-verified** (the controller runs it; the agent does not spin up a VM).
- **iocraft pin (recorded).** `iocraft = "=0.8.2"` with the `unstable-output-streams` feature (the devenv workspace's line). devenv git-patches iocraft to `main` for stderr rendering; if released 0.8.2's stderr sink is missing the API golemctl needs (`render_loop`/element render to stderr), add the same `[patch.crates-io] iocraft = { git = "https://github.com/ccbrown/iocraft", branch = "main" }` — the `0.8` line is the commitment, the git patch is a build-out contingency.

---

## File Structure

**golemd (protocol first):**
- `apps/golemd/src/progress.rs` — **new.** The bounded per-attempt in-memory event ring: `ProgressEvent`, `EventLevel`, an `EventRing` (seq counter + `VecDeque` cap), a `ProgressRegistry` (attempt id → ring) behind a `Mutex`, and the `record_delay`/`record_reason` writer helpers the enact spine calls. Also `next_retry_in_ms` live state per attempt.
- `apps/golemd/src/projection.rs` — **new.** The pure WAL-fold-to-projection: `ReconcileProgress`, `UnitProgress`, `GlyphProgress`, `GlyphState`, `PhaseView`, and `project(attempt, steps, events, after) -> ReconcileProgress`. No I/O — takes the attempt, its steps, and the event slice and folds. This is where per-glyph `pending`/`in_progress`/terminal states are derived.
- `apps/golemd/src/foreman.rs` — **modify.** Split `apply_manifest` into a synchronous `ingest(bytes) -> Result<u64, ForemanError>` (decode + select + recover + gate + open attempt) and a `run_reconcile(reconcile_id, selected)` that enacts (the current `reconcile` body from the attempt onward). Add the `ForemanError::ReconcileInProgress { reconcile_id }` variant. Give `Foreman` a `ProgressRegistry` field. Emit progress events at the `enact_apply`/`enact_reverse`/round-loop call sites.
- `apps/golemd/src/http.rs` — **modify.** `apply_manifest` handler becomes 202-returning; add `reconcile`/`reconcile_latest` GET handlers; wire the two routes; map `ReconcileInProgress` to 409.
- `apps/golemd/src/lib.rs` — **modify.** `pub mod progress; pub mod projection;`.
- `apps/golemd/tests/report_api.rs` — **modify.** Migrate the two HTTP tests from sync-200-body to 202-then-poll.
- `apps/golemd/tests/async_apply.rs` — **new.** End-to-end 202 + poll-to-settle + 409-conflict integration test over a real axum server.

**golemctl (async client + model + view):**
- `apps/golemctl/Cargo.toml` — **modify.** Add `iocraft` (and `chrono` for timestamps if needed).
- `apps/golemctl/src/poll.rs` — **new.** The typed poll client: `Poll` response structs mirroring the JSON, `post_manifest(addr, bytes) -> Reconcile202`, `get_reconcile(addr, id, after) -> ReconcileProgress`, and the settle-detection helper.
- `apps/golemctl/src/model.rs` — **new.** The TUI model: `ApplyModel` (unit tree keyed by `unit_path`), `UnitNode`, `GlyphRow`, `UnitState`, plus `apply_progress(&mut self, ReconcileProgress)` that folds one poll response into the model and appends events to the active node's log ring.
- `apps/golemctl/src/view.rs` — **new.** The pure iocraft `view(&ApplyModel) -> impl Into<AnyElement>` plus the `Spinner`/`StatusIndicator` components (SPINNER_FRAMES, CHECKMARK/XMARK), adapted from devenv-tui.
- `apps/golemctl/src/apply.rs` — **new.** The apply orchestration: TTY-detect, fire-then-poll loop, drive the iocraft render loop on a tty, or the plain-line/`--json` fallback otherwise, then print the final report (reusing the existing pretty-printer).
- `apps/golemctl/src/main.rs` — **modify.** Wire the new `--json`/`--reattach` flags and call into `apply`.
- `apps/golemctl/tests/model_tests.rs` — **new.** Model-level tests (devenv `tui_tests.rs` pattern): apply projection snapshots, assert the rendered view string.

**fleet (delegation + stopgap removal):**
- `apps/fleet/deploy.py` — **modify.** Add `resolve_golemctl(paths) -> Path`.
- `apps/fleet/cli.py` — **modify.** Replace the per-host HTTP apply body with a `golemctl apply` exec; delete `_render_apply_transport_error`; retire `_render_report` from the apply path (kept as a function only if a later task needs it — this plan removes its apply call).
- `apps/fleet/golemd_client.py` — **modify.** Delete `apply_manifest` and `_APPLY_TIMEOUT`; keep `status`/`state`.
- `apps/fleet/tests/test_apply_render.py` — **modify.** Migrate the transport-error/continue tests to the golemctl-exec shape.

---

### Task 1: golemd — split ingest from run, add the ReconcileInProgress error, keep the sync `apply_manifest` test-only shim

**Files:**
- Modify: `apps/golemd/src/foreman.rs` (`apply_manifest`, `reconcile`, `ForemanError`)
- Test: `apps/golemd/src/foreman.rs` (the existing `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: nothing new (uses existing `PlanRoom`, `SelectedScroll`, `from_bytes`, `select`).
- Produces:
  - `pub fn ingest(&self, bytes: &[u8]) -> Result<(u64, SelectedScroll), ForemanError>` — does decode + `select` + `recover_locked` + the unsettled-attempt gate + `open_attempt` + set phase `Enacting`, returns `(reconcile_id, selected)`. Holds and releases the write lock **only** for recover+gate+open (it does not hold it across the enact).
  - `pub fn run_reconcile(&self, reconcile_id: u64, desired: SelectedScroll) -> Result<ReconcileReport, ForemanError>` — takes the write lock and runs the enact loop, config propagation, settle, and roll-up (the current `reconcile` body from `let prior = …` onward, minus the attempt-open which `ingest` now does).
  - `pub fn apply_manifest(&self, bytes: &[u8]) -> Result<ReconcileReport, ForemanError>` — kept as a **synchronous convenience** used by the foreman unit/integration tests: `let (id, sel) = self.ingest(bytes)?; self.run_reconcile(id, sel)`.
  - `ForemanError::ReconcileInProgress { reconcile_id: u64 }` — new variant; `kind()` returns `"reconcile-in-progress"`, `message()` returns `format!("a reconcile ({reconcile_id}) is already running on this host; poll it instead of re-applying")`.

> **Note on the write lock:** today `reconcile` holds `write` for recover+gate+open **and** the whole enact. After this split, `ingest` takes `write` for recover+gate+open then drops it, and `run_reconcile` re-takes `write` for the enact. Between those two lock acquisitions no other reconcile can start because the attempt is already open and unsettled — a concurrent `ingest` will hit the gate and return `ReconcileInProgress`. This preserves the ADR 0020 one-attempt-at-a-time invariant.

- [ ] **Step 1: Write the failing test** — a second ingest while an attempt is unsettled is a typed conflict.

Add to `apps/golemd/src/foreman.rs` `mod tests`:

```rust
#[test]
fn a_second_ingest_while_unsettled_is_reconcile_in_progress() {
    let f = foreman_with(ScriptedReconciler::new().ok_default());
    let scroll = Scroll {
        name: "host".into(),
        policy: None,
        contents: Contents::Glyphs(vec![apt("nginx")]),
    };
    let bytes = scroll_format::to_bytes(&Manifest::from_scrolls(vec![scroll], "test"));
    let (id, _sel) = f.foreman.ingest(&bytes).unwrap();
    assert_eq!(id, 1);
    let err = f.foreman.ingest(&bytes).unwrap_err();
    assert!(matches!(
        err,
        ForemanError::ReconcileInProgress { reconcile_id: 1 }
    ));
    assert_eq!(err.kind(), "reconcile-in-progress");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p golemd --lib a_second_ingest_while_unsettled_is_reconcile_in_progress`
Expected: FAIL — `ingest` and `ReconcileInProgress` do not exist (compile error).

- [ ] **Step 3: Implement the split and the new error variant**

In `apps/golemd/src/foreman.rs`, add the variant to `ForemanError`:

```rust
#[derive(Debug)]
pub enum ForemanError {
    WalUnreadable { detail: String },
    ManifestUndecodable { detail: String },
    ReconcileInProgress { reconcile_id: u64 },
    Internal(anyhow::Error),
}
```

Extend `kind` and `message`:

```rust
    pub fn kind(&self) -> &'static str {
        match self {
            ForemanError::WalUnreadable { .. } => "wal-unreadable",
            ForemanError::ManifestUndecodable { .. } => "manifest-undecodable",
            ForemanError::ReconcileInProgress { .. } => "reconcile-in-progress",
            ForemanError::Internal(_) => "internal",
        }
    }

    pub fn message(&self) -> String {
        match self {
            ForemanError::WalUnreadable { .. } => {
                "golemd couldn't read its write-ahead log; it may be from an incompatible golemd version. Run `fleet reset` on this host to start from a clean state.".to_string()
            }
            ForemanError::ManifestUndecodable { detail } => {
                format!("golemd couldn't decode the manifest: {detail}")
            }
            ForemanError::ReconcileInProgress { reconcile_id } => {
                format!("a reconcile ({reconcile_id}) is already running on this host; poll it instead of re-applying")
            }
            ForemanError::Internal(e) => format!("{e:#}"),
        }
    }
```

Replace `apply_manifest` and `reconcile` with `ingest` + `run_reconcile` + a shim. The `ingest` gate now returns `ReconcileInProgress`:

```rust
    pub fn apply_manifest(&self, bytes: &[u8]) -> Result<ReconcileReport, ForemanError> {
        let (reconcile_id, selected) = self.ingest(bytes)?;
        self.run_reconcile(reconcile_id, selected)
    }

    pub fn ingest(&self, bytes: &[u8]) -> Result<(u64, SelectedScroll), ForemanError> {
        let manifest = from_bytes(bytes).map_err(|e| ForemanError::ManifestUndecodable {
            detail: e.to_string(),
        })?;
        let selected = self.select(&manifest.scrolls);
        info!(
            host = %self.host,
            scroll = %selected.content_id,
            glyphs = selected.scroll.all_glyphs().len(),
            "manifest ingested"
        );
        let _w = self.write.lock().unwrap();
        self.recover_locked()
            .map_err(|e| ForemanError::WalUnreadable {
                detail: e.to_string(),
            })?;
        if let Some(attempt) =
            self.planroom
                .latest_attempt()
                .map_err(|e| ForemanError::WalUnreadable {
                    detail: e.to_string(),
                })?
        {
            if !attempt.phase.is_settled() {
                return Err(ForemanError::ReconcileInProgress {
                    reconcile_id: attempt.reconcile_id,
                });
            }
        }
        let attempt = self
            .planroom
            .open_attempt(Some(selected.content_id))
            .map_err(ForemanError::Internal)?;
        self.planroom
            .set_attempt_phase(attempt.reconcile_id, AttemptPhase::Enacting)
            .map_err(ForemanError::Internal)?;
        Ok((attempt.reconcile_id, selected))
    }

    pub fn run_reconcile(
        &self,
        reconcile_id: u64,
        desired: SelectedScroll,
    ) -> Result<ReconcileReport, ForemanError> {
        let _w = self.write.lock().unwrap();
        let steps = self
            .planroom
            .wal_steps()
            .map_err(|e| ForemanError::WalUnreadable {
                detail: e.to_string(),
            })?;
        let prior = applied_outcomes(&steps);
        let retry_clock: Cell<Option<Instant>> = Cell::new(None);
        let units = desired.scroll.leaf_units();
        let mut unit_reports = Vec::new();
        let mut next_ord: u64 = 0;
        for unit in &units {
            let effective = resolve_retry(&self.retry, &unit.policy_chain);
            let ops: Vec<GlyphOp> = plan(&prior, &leaf_as_scroll(unit))
                .into_iter()
                .filter(|o| !matches!(o, GlyphOp::Remove { .. }))
                .collect();
            let result = self
                .enact_unit(
                    reconcile_id,
                    &mut next_ord,
                    &ops,
                    &prior,
                    &unit.path,
                    &effective,
                    &retry_clock,
                )
                .map_err(ForemanError::Internal)?;
            unit_reports.push(unit_report_from(result));
        }
        for group in self
            .plan_vanished_removes(&prior, &desired.scroll, &units)
            .map_err(ForemanError::Internal)?
        {
            let effective = resolve_retry(&self.retry, &group.policy_chain);
            let result = self
                .enact_unit(
                    reconcile_id,
                    &mut next_ord,
                    &group.ops,
                    &prior,
                    &group.unit_path,
                    &effective,
                    &retry_clock,
                )
                .map_err(ForemanError::Internal)?;
            unit_reports.push(unit_report_from(result));
        }
        self.propagate_config(reconcile_id)
            .map_err(ForemanError::Internal)?;
        let revision = self
            .settle(reconcile_id, &desired)
            .map_err(ForemanError::Internal)?;
        let report = ReconcileReport::roll_up(revision, unit_reports);
        log_settled(&report);
        Ok(report)
    }
```

Delete the old private `reconcile` method (its body moved into `ingest`+`run_reconcile`). `settle` takes `reconcile_id` (it already does — its signature is `settle(&self, reconcile_id: u64, desired: &SelectedScroll)`), so `run_reconcile` passes `reconcile_id`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p golemd --lib a_second_ingest_while_unsettled_is_reconcile_in_progress`
Expected: PASS.

- [ ] **Step 5: Run the whole golemd lib+integration suite to confirm the split preserved behaviour**

Run: `cargo test -p golemd`
Expected: PASS — every existing foreman/wal/recovery/report test still green (the sync `apply_manifest` shim keeps them working). If `report_api.rs`'s HTTP tests fail here, that is expected and fixed in Task 5; leave them for now by running `cargo test -p golemd --lib` if you want a green gate for this task.

- [ ] **Step 6: Commit**

```bash
git add apps/golemd/src/foreman.rs
git commit -m "refactor(golemd): split manifest ingest from reconcile run; add ReconcileInProgress

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog (for the documenter):** the two-lock-acquisition invariant on `ingest`/`run_reconcile` (why dropping the write lock between them is safe — the open unsettled attempt gates a concurrent ingest); the `ReconcileInProgress` variant's client contract (409, carries the pollable id).

---

### Task 2: golemd — the projection module (pure WAL fold to per-glyph progress)

**Files:**
- Create: `apps/golemd/src/projection.rs`
- Modify: `apps/golemd/src/lib.rs` (add `pub mod projection;`)
- Test: `apps/golemd/src/projection.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `crate::journal::{ReconcileAttempt, AttemptPhase, WalStep, WalStepState, WalAction, GlyphOp}`, `crate::report::ReconcileReport`, and (Task 3) `crate::progress::ProgressEvent`.
- Produces:
  - `pub enum PhaseView { Planning, Enacting, Settling, Settled, RolledBack }` with `#[serde(rename_all = "snake_case")]`, and `pub fn phase_view(p: AttemptPhase) -> PhaseView` mapping `Planning→Planning`, `Enacting→Enacting`, `RollingBack→RolledBack`, `Committed→Settled`, `RolledBack→RolledBack`. (There is no distinct `settling` phase in `AttemptPhase`; `Settling` is reserved for a future config-propagation phase and is emitted only if such a phase is ever added — for now `Committed` maps to `Settled`. Recorded honestly: the ADR lists `settling` but the current `AttemptPhase` has no such value, so the projection never emits it today.)
  - `pub enum GlyphState { Pending, InProgress, Applied, Unchanged, Failed, RolledBack }` `#[serde(rename_all = "snake_case")]`.
  - `pub struct GlyphProgress { pub glyph_key: String, pub action: String, pub state: GlyphState, pub rounds: u32, pub next_retry_in_ms: Option<u64> }`.
  - `pub struct UnitProgress { pub unit_path: Vec<String>, pub glyphs: Vec<GlyphProgress> }`.
  - `pub struct ReconcileProgress { pub reconcile_id: u64, pub phase: PhaseView, pub units: Vec<UnitProgress>, pub events: Vec<crate::progress::ProgressEvent>, pub cursor: u64, pub report: Option<ReconcileReport> }` — all `#[derive(Serialize)]`.
  - `pub fn project(attempt: &ReconcileAttempt, steps: &[WalStep], events: Vec<crate::progress::ProgressEvent>, report: Option<ReconcileReport>, retries: &std::collections::BTreeMap<String, u64>) -> ReconcileProgress` — folds `steps` into per-`unit_path` per-`glyph_key` `GlyphProgress`, sets `cursor` to the max event `seq` (or the `after` the caller already advanced past — the caller passes only events `> after`, and sets `cursor` to the last one or leaves it at `after`), and stamps `next_retry_in_ms` from `retries` keyed by glyph_key.

> **Task-3 forward reference:** `ProgressEvent` (from `crate::progress`) is defined in Task 3. To keep Task 2 self-contained and testable first, define `project` to take `events: Vec<crate::progress::ProgressEvent>` — but Task 2's *tests* pass an empty `vec![]` and a `BTreeMap::new()`, so Task 2 compiles against Task 3's type. **Order of implementation: do Task 3's type definition first if the compiler complains** — or, simpler, land Task 3's `progress.rs` `ProgressEvent`/`EventLevel` structs in this same task's first step (they are tiny plain structs). This plan defines them in Task 3; if executing strictly in order, add the `progress` module stub (just the two structs) at the top of Task 2 Step 3.

- [ ] **Step 1: Write the failing test** — fold a two-glyph attempt's WAL into per-unit progress.

Create `apps/golemd/src/projection.rs` ending with:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::{GlyphOp, ReconcileAttempt, WalAction, WalStep, WalStepState};
    use chrono::Utc;
    use scroll_format::Glyph;
    use std::collections::BTreeMap;

    fn apt_op(name: &str) -> GlyphOp {
        let glyph = Glyph::AptPackage { name: name.into() };
        GlyphOp::Install {
            cid: scroll_format::content_id_of_glyph(&glyph),
            glyph,
        }
    }

    fn step(seq: u64, ord: u64, key: &str, state: WalStepState, unit: &[&str]) -> WalStep {
        WalStep {
            seq,
            reconcile_id: 1,
            step_ord: ord,
            glyph_key: key.into(),
            action: WalAction::Apply,
            state,
            op: apt_op(key.trim_start_matches("apt:")),
            inverse: None,
            changed: None,
            unit_path: unit.iter().map(|s| s.to_string()).collect(),
            at: Utc::now(),
        }
    }

    fn attempt(phase: AttemptPhase) -> ReconcileAttempt {
        ReconcileAttempt {
            reconcile_id: 1,
            started_at: Utc::now(),
            scroll_content_id: None,
            phase,
            settled_at: None,
        }
    }

    #[test]
    fn a_done_glyph_projects_applied_and_a_bare_intended_is_in_progress() {
        let steps = vec![
            step(1, 0, "apt:nginx", WalStepState::Intended, &["scaly", "a"]),
            step(2, 0, "apt:nginx", WalStepState::Done, &["scaly", "a"]),
            step(3, 1, "apt:pg", WalStepState::Intended, &["scaly", "a"]),
        ];
        let p = project(
            &attempt(AttemptPhase::Enacting),
            &steps,
            vec![],
            None,
            &BTreeMap::new(),
        );
        assert!(matches!(p.phase, PhaseView::Enacting));
        assert_eq!(p.units.len(), 1);
        assert_eq!(p.units[0].unit_path, vec!["scaly", "a"]);
        let nginx = p.units[0].glyphs.iter().find(|g| g.glyph_key == "apt:nginx").unwrap();
        assert!(matches!(nginx.state, GlyphState::Applied));
        let pg = p.units[0].glyphs.iter().find(|g| g.glyph_key == "apt:pg").unwrap();
        assert!(matches!(pg.state, GlyphState::InProgress));
        assert!(p.report.is_none());
    }

    #[test]
    fn a_committed_attempt_projects_settled_with_the_report() {
        let json = serde_json::to_value(&PhaseView::from(phase_view(AttemptPhase::Committed))).unwrap();
        assert_eq!(json, "settled");
    }

    #[test]
    fn repeated_intended_failed_brackets_count_rounds() {
        let steps = vec![
            step(1, 0, "apt:x", WalStepState::Intended, &["scaly"]),
            step(2, 0, "apt:x", WalStepState::Failed, &["scaly"]),
            step(3, 0, "apt:x", WalStepState::Intended, &["scaly"]),
            step(4, 0, "apt:x", WalStepState::Failed, &["scaly"]),
        ];
        let p = project(&attempt(AttemptPhase::Enacting), &steps, vec![], None, &BTreeMap::new());
        let g = &p.units[0].glyphs[0];
        assert!(matches!(g.state, GlyphState::Failed));
        assert_eq!(g.rounds, 2);
    }
}
```

(The middle test's `PhaseView::from(...)` line is a lazy convenience — replace with `serde_json::to_value(&phase_view(AttemptPhase::Committed)).unwrap()` and drop the `From` impl; it is only asserting the snake_case tag.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p golemd --lib projection::`
Expected: FAIL — `projection` module and its types do not exist.

- [ ] **Step 3: Implement the projection**

Add `pub mod projection;` to `apps/golemd/src/lib.rs` (alphabetically between `planroom` and `reconcile`). Then write `apps/golemd/src/projection.rs`:

```rust
use std::collections::BTreeMap;

use serde::Serialize;

use crate::journal::{AttemptPhase, GlyphOp, ReconcileAttempt, WalAction, WalStep, WalStepState};
use crate::progress::ProgressEvent;
use crate::report::ReconcileReport;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PhaseView {
    Planning,
    Enacting,
    Settling,
    Settled,
    RolledBack,
}

pub fn phase_view(phase: AttemptPhase) -> PhaseView {
    match phase {
        AttemptPhase::Planning => PhaseView::Planning,
        AttemptPhase::Enacting => PhaseView::Enacting,
        AttemptPhase::RollingBack => PhaseView::RolledBack,
        AttemptPhase::Committed => PhaseView::Settled,
        AttemptPhase::RolledBack => PhaseView::RolledBack,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GlyphState {
    Pending,
    InProgress,
    Applied,
    Unchanged,
    Failed,
    RolledBack,
}

#[derive(Debug, Clone, Serialize)]
pub struct GlyphProgress {
    pub glyph_key: String,
    pub action: String,
    pub state: GlyphState,
    pub rounds: u32,
    pub next_retry_in_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnitProgress {
    pub unit_path: Vec<String>,
    pub glyphs: Vec<GlyphProgress>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReconcileProgress {
    pub reconcile_id: u64,
    pub phase: PhaseView,
    pub units: Vec<UnitProgress>,
    pub events: Vec<ProgressEvent>,
    pub cursor: u64,
    pub report: Option<ReconcileReport>,
}

fn action_tag(op: &GlyphOp) -> &'static str {
    match op {
        GlyphOp::Install { .. } => "install",
        GlyphOp::Replace { .. } => "replace",
        GlyphOp::Remove { .. } => "remove",
        GlyphOp::Noop { .. } => "noop",
    }
}

fn fold_state(rows: &[&WalStep]) -> (GlyphState, u32) {
    let mut rounds = 0u32;
    let mut last_terminal: Option<WalStepState> = None;
    let mut saw_intended_without_terminal = false;
    let mut i = 0;
    while i < rows.len() {
        match rows[i].state {
            WalStepState::Intended => {
                let mut j = i + 1;
                let mut terminal = None;
                while j < rows.len() {
                    match rows[j].state {
                        WalStepState::Done | WalStepState::Failed | WalStepState::Reversed => {
                            terminal = Some(rows[j].state);
                            break;
                        }
                        _ => j += 1,
                    }
                }
                match terminal {
                    Some(WalStepState::Failed) => {
                        rounds += 1;
                        last_terminal = Some(WalStepState::Failed);
                        i = j + 1;
                    }
                    Some(t) => {
                        last_terminal = Some(t);
                        i = j + 1;
                    }
                    None => {
                        saw_intended_without_terminal = true;
                        i += 1;
                    }
                }
            }
            _ => i += 1,
        }
    }
    let state = if saw_intended_without_terminal {
        GlyphState::InProgress
    } else {
        match last_terminal {
            Some(WalStepState::Done) => GlyphState::Applied,
            Some(WalStepState::Failed) => GlyphState::Failed,
            Some(WalStepState::Reversed) => GlyphState::RolledBack,
            _ => GlyphState::Pending,
        }
    };
    (state, rounds.max(if matches!(state, GlyphState::Failed) { 1 } else { 0 }))
}

pub fn project(
    attempt: &ReconcileAttempt,
    steps: &[WalStep],
    events: Vec<ProgressEvent>,
    report: Option<ReconcileReport>,
    retries: &BTreeMap<String, u64>,
) -> ReconcileProgress {
    let mut order: Vec<Vec<String>> = Vec::new();
    let mut by_unit: BTreeMap<Vec<String>, Vec<String>> = BTreeMap::new();
    let mut rows_by_key: BTreeMap<(Vec<String>, String), Vec<&WalStep>> = BTreeMap::new();
    for step in steps.iter().filter(|s| s.reconcile_id == attempt.reconcile_id) {
        if step.action == WalAction::Restart {
            continue;
        }
        let unit = step.unit_path.clone();
        if !order.contains(&unit) {
            order.push(unit.clone());
        }
        let keys = by_unit.entry(unit.clone()).or_default();
        if !keys.contains(&step.glyph_key) {
            keys.push(step.glyph_key.clone());
        }
        rows_by_key
            .entry((unit, step.glyph_key.clone()))
            .or_default()
            .push(step);
    }
    let mut units = Vec::new();
    for unit in &order {
        let mut glyphs = Vec::new();
        for key in &by_unit[unit] {
            let rows = &rows_by_key[&(unit.clone(), key.clone())];
            let (state, rounds) = fold_state(rows);
            let action = rows
                .last()
                .map(|s| action_tag(&s.op).to_string())
                .unwrap_or_else(|| "install".into());
            let state = if action == "noop" && matches!(state, GlyphState::Applied) {
                GlyphState::Unchanged
            } else {
                state
            };
            glyphs.push(GlyphProgress {
                glyph_key: key.clone(),
                action,
                state,
                rounds,
                next_retry_in_ms: retries.get(key).copied(),
            });
        }
        units.push(UnitProgress {
            unit_path: unit.clone(),
            glyphs,
        });
    }
    let cursor = events.iter().map(|e| e.seq).max().unwrap_or(0);
    ReconcileProgress {
        reconcile_id: attempt.reconcile_id,
        phase: phase_view(attempt.phase),
        units,
        events,
        cursor,
        report,
    }
}
```

(The `cursor` here is derived from the events slice's max seq; the HTTP handler in Task 4 overrides it to `max(after, that)` so a poll with no new events keeps the client's cursor. See Task 4's `reconcile` handler.)

- [ ] **Step 4: Fix the middle test** — replace its lazy line with the real assertion:

```rust
    #[test]
    fn committed_phase_serializes_as_settled() {
        let v = serde_json::to_value(&phase_view(AttemptPhase::Committed)).unwrap();
        assert_eq!(v, "settled");
    }
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p golemd --lib projection::`
Expected: PASS — but this requires `crate::progress::ProgressEvent` to exist. If it does not yet, land Task 3 first (or add the `ProgressEvent`/`EventLevel` structs now — they are shown in Task 3 Step 3). Prefer executing **Task 3 before Task 2's Step 5**.

- [ ] **Step 6: Commit**

```bash
git add apps/golemd/src/projection.rs apps/golemd/src/lib.rs
git commit -m "feat(golemd): WAL-to-progress projection (per-glyph pending/in_progress/terminal states)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** why `Settling` is defined but never emitted today (no `AttemptPhase::Settling`); the `pending`/`in_progress` projection-only states vs the report's terminal vocabulary; how `rounds` counts `Intended→Failed` brackets; that `next_retry_in_ms` is the one non-durable seam.

---

### Task 3: golemd — the in-memory per-attempt event ring + live retry-countdown state

**Files:**
- Create: `apps/golemd/src/progress.rs`
- Modify: `apps/golemd/src/lib.rs` (add `pub mod progress;`)
- Test: `apps/golemd/src/progress.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: nothing (leaf module: `serde`, `chrono`, `std`).
- Produces:
  - `pub enum EventLevel { Info, Warn, Error }` `#[serde(rename_all = "snake_case")]`.
  - `pub struct ProgressEvent { pub seq: u64, pub at: chrono::DateTime<chrono::Utc>, pub level: EventLevel, pub unit_path: Vec<String>, pub glyph_key: String, pub message: String }` `#[derive(Serialize, Clone, Debug)]`.
  - `pub struct ProgressRegistry` behind a `Mutex` internally: `new()`, `open(reconcile_id)`, `record(reconcile_id, level, unit_path, glyph_key, message)` (appends to the ring, assigns the next seq, drops oldest past the cap), `set_retry(reconcile_id, glyph_key, ms)` / `clear_retry(reconcile_id, glyph_key)`, `events_after(reconcile_id, after) -> Vec<ProgressEvent>`, `retries(reconcile_id) -> BTreeMap<String, u64>`, and `close(reconcile_id)` (drops the ring on settle to bound memory — or keeps it for a short reattach window; see below).
  - `pub const EVENT_RING_CAP: usize = 1024;` — the per-attempt bound (ADR open question: low thousands, devenv's `max_build_logs` default of 1000 per node — golem uses 1024 per attempt).

> **Retention (ADR open question, decided here):** `close` **keeps** the ring in the registry (does not drop it) so a client that polls just after settle still gets the final event lines. The registry keeps only the **latest** attempt's ring plus the one before it (a 2-entry LRU by reconcile_id), which bounds memory while covering a reattach. State durability is unaffected — the WAL-derived states survive restart regardless; only these transient lines are best-effort and LRU-bounded.

- [ ] **Step 1: Write the failing test** — a ring assigns monotonic seqs and serves the `> after` slice, dropping past the cap.

Create `apps/golemd/src/progress.rs` ending with:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_get_monotonic_seqs_and_after_returns_only_newer() {
        let reg = ProgressRegistry::new();
        reg.open(1);
        reg.record(1, EventLevel::Info, &["scaly".into()], "apt:nginx", "install apt:nginx");
        reg.record(1, EventLevel::Warn, &["scaly".into()], "apt:nginx", "enact failed (round 1)");
        let all = reg.events_after(1, 0);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].seq, 1);
        assert_eq!(all[1].seq, 2);
        let tail = reg.events_after(1, 1);
        assert_eq!(tail.len(), 1);
        assert_eq!(tail[0].seq, 2);
        assert!(matches!(tail[0].level, EventLevel::Warn));
    }

    #[test]
    fn the_ring_drops_oldest_past_the_cap_but_keeps_seq_monotone() {
        let reg = ProgressRegistry::new();
        reg.open(1);
        for i in 0..(EVENT_RING_CAP as u64 + 5) {
            reg.record(1, EventLevel::Info, &["scaly".into()], "apt:x", &format!("line {i}"));
        }
        let all = reg.events_after(1, 0);
        assert_eq!(all.len(), EVENT_RING_CAP);
        assert_eq!(all.last().unwrap().seq, EVENT_RING_CAP as u64 + 5);
        assert!(all.first().unwrap().seq > 1);
    }

    #[test]
    fn retry_countdown_is_set_and_cleared_per_glyph() {
        let reg = ProgressRegistry::new();
        reg.open(1);
        reg.set_retry(1, "apt:x", 2000);
        assert_eq!(reg.retries(1).get("apt:x").copied(), Some(2000));
        reg.clear_retry(1, "apt:x");
        assert!(reg.retries(1).get("apt:x").is_none());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p golemd --lib progress::`
Expected: FAIL — module missing.

- [ ] **Step 3: Implement the ring and registry**

Add `pub mod progress;` to `apps/golemd/src/lib.rs` (alphabetically after `planroom`, before `projection`). Then write `apps/golemd/src/progress.rs`:

```rust
use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use serde::Serialize;

pub const EVENT_RING_CAP: usize = 1024;
const ATTEMPT_LRU: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventLevel {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProgressEvent {
    pub seq: u64,
    pub at: DateTime<Utc>,
    pub level: EventLevel,
    pub unit_path: Vec<String>,
    pub glyph_key: String,
    pub message: String,
}

struct AttemptRing {
    next_seq: u64,
    events: VecDeque<ProgressEvent>,
    retries: BTreeMap<String, u64>,
}

impl AttemptRing {
    fn new() -> Self {
        Self {
            next_seq: 1,
            events: VecDeque::new(),
            retries: BTreeMap::new(),
        }
    }
}

struct Inner {
    rings: BTreeMap<u64, AttemptRing>,
    order: VecDeque<u64>,
}

pub struct ProgressRegistry {
    inner: Mutex<Inner>,
}

impl ProgressRegistry {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                rings: BTreeMap::new(),
                order: VecDeque::new(),
            }),
        }
    }

    pub fn open(&self, reconcile_id: u64) {
        let mut inner = self.inner.lock().unwrap();
        if !inner.rings.contains_key(&reconcile_id) {
            inner.rings.insert(reconcile_id, AttemptRing::new());
            inner.order.push_back(reconcile_id);
            while inner.order.len() > ATTEMPT_LRU {
                if let Some(evicted) = inner.order.pop_front() {
                    inner.rings.remove(&evicted);
                }
            }
        }
    }

    pub fn record(
        &self,
        reconcile_id: u64,
        level: EventLevel,
        unit_path: &[String],
        glyph_key: &str,
        message: &str,
    ) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(ring) = inner.rings.get_mut(&reconcile_id) {
            let seq = ring.next_seq;
            ring.next_seq += 1;
            ring.events.push_back(ProgressEvent {
                seq,
                at: Utc::now(),
                level,
                unit_path: unit_path.to_vec(),
                glyph_key: glyph_key.to_string(),
                message: message.to_string(),
            });
            while ring.events.len() > EVENT_RING_CAP {
                ring.events.pop_front();
            }
        }
    }

    pub fn set_retry(&self, reconcile_id: u64, glyph_key: &str, ms: u64) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(ring) = inner.rings.get_mut(&reconcile_id) {
            ring.retries.insert(glyph_key.to_string(), ms);
        }
    }

    pub fn clear_retry(&self, reconcile_id: u64, glyph_key: &str) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(ring) = inner.rings.get_mut(&reconcile_id) {
            ring.retries.remove(glyph_key);
        }
    }

    pub fn events_after(&self, reconcile_id: u64, after: u64) -> Vec<ProgressEvent> {
        let inner = self.inner.lock().unwrap();
        match inner.rings.get(&reconcile_id) {
            Some(ring) => ring
                .events
                .iter()
                .filter(|e| e.seq > after)
                .cloned()
                .collect(),
            None => Vec::new(),
        }
    }

    pub fn retries(&self, reconcile_id: u64) -> BTreeMap<String, u64> {
        let inner = self.inner.lock().unwrap();
        inner
            .rings
            .get(&reconcile_id)
            .map(|r| r.retries.clone())
            .unwrap_or_default()
    }
}

impl Default for ProgressRegistry {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p golemd --lib progress::`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/golemd/src/progress.rs apps/golemd/src/lib.rs
git commit -m "feat(golemd): bounded per-attempt progress event ring + retry-countdown state

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** why the ring is best-effort and lost on restart (vs WAL-durable states); the 2-attempt LRU retention choice and the 1024 cap (ADR open question resolutions); that `record` at a dropped/closed attempt is a silent no-op.

---

### Task 4: golemd — wire progress emission into the enact spine and give the Foreman a registry

**Files:**
- Modify: `apps/golemd/src/foreman.rs` (`Foreman` struct + `new` + `ingest` + `enact_apply`/`enact_reverse`/round loop; add a `progress()` accessor)
- Test: `apps/golemd/src/foreman.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `crate::progress::{ProgressRegistry, EventLevel}`.
- Produces:
  - `Foreman` gains `progress: ProgressRegistry` and `pub fn progress(&self) -> &ProgressRegistry`.
  - `ingest` calls `self.progress.open(reconcile_id)` right after `open_attempt`.
  - The enact spine records events at the same points it already `tracing`-logs: at each `enact_apply`/`enact_reverse` `Intended` it records an `Info` "`<action> <glyph_key>`"; at each `Failed` it records a `Warn` (retryable) or `Error` (fatal) carrying the reason; before each retry sleep it `set_retry(reconcile_id, key, delay_ms)` for the still-failing keys and `clear_retry` once re-driven.

> **Deliberate design (recorded):** events are emitted **explicitly at the enact call sites**, not via a tracing `Layer`. This is simpler and typed — the call site already has the `reconcile_id`, `unit_path`, `glyph_key`, and reason in hand, so a typed `record(...)` is a one-liner beside the existing `info!`/`warn!`, with no subscriber plumbing, no string re-parsing, and no coupling of the event schema to log formatting.

- [ ] **Step 1: Write the failing test** — an apply emits install events observable on the registry.

Add to `apps/golemd/src/foreman.rs` `mod tests`:

```rust
#[test]
fn an_apply_emits_progress_events_for_each_installed_glyph() {
    let f = foreman_with(ScriptedReconciler::new().ok_default());
    let scroll = Scroll {
        name: "host".into(),
        policy: None,
        contents: Contents::Glyphs(vec![apt("nginx"), apt("pg")]),
    };
    let bytes = scroll_format::to_bytes(&Manifest::from_scrolls(vec![scroll], "test"));
    let (id, sel) = f.foreman.ingest(&bytes).unwrap();
    f.foreman.run_reconcile(id, sel).unwrap();
    let events = f.foreman.progress().events_after(id, 0);
    let messages: Vec<&str> = events.iter().map(|e| e.message.as_str()).collect();
    assert!(messages.iter().any(|m| m.contains("apt:nginx")));
    assert!(messages.iter().any(|m| m.contains("apt:pg")));
    assert!(events.iter().all(|e| e.seq >= 1));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p golemd --lib an_apply_emits_progress_events_for_each_installed_glyph`
Expected: FAIL — `progress()` accessor and emission do not exist.

- [ ] **Step 3: Add the registry field and emit events**

In the `Foreman` struct add `progress: ProgressRegistry,`. In `Foreman::new` initialize `progress: ProgressRegistry::new(),`. Add the accessor:

```rust
    pub fn progress(&self) -> &crate::progress::ProgressRegistry {
        &self.progress
    }
```

Add `use crate::progress::{EventLevel, ProgressRegistry};` to the imports. In `ingest`, right after the `set_attempt_phase(... Enacting)` call and before `Ok((...))`:

```rust
        self.progress.open(attempt.reconcile_id);
```

Emit at the `Intended` of `enact_apply` (right after the `append_wal_step(... Intended ...)?;`):

```rust
        self.progress.record(
            reconcile_id,
            EventLevel::Info,
            unit_path,
            &op.key(),
            &format!("{} {}", action_tag_for(op), op.key()),
        );
```

where `action_tag_for` is a tiny free fn added near `glyph_action_of`:

```rust
fn action_tag_for(op: &GlyphOp) -> &'static str {
    match op {
        GlyphOp::Install { .. } => "install",
        GlyphOp::Replace { .. } => "replace",
        GlyphOp::Remove { .. } => "remove",
        GlyphOp::Noop { .. } => "noop",
    }
}
```

In the `Err(e)` arm of `enact_apply` (after the existing `log_step_failure`), record the reason:

```rust
                let (level, reason) = match &class {
                    StepClass::Failed(RetryClass::Retryable, m) => (EventLevel::Warn, m.clone()),
                    StepClass::Failed(RetryClass::Fatal, m) => (EventLevel::Error, m.clone()),
                    StepClass::Ok => (EventLevel::Info, String::new()),
                };
                self.progress
                    .record(reconcile_id, level, unit_path, &op.key(), &reason);
```

Do the same in `enact_reverse`'s `Err` arm. (These arms already compute `class` and call `log_step_failure`; add the `record` beside it.)

In `enact_unit`'s round loop, before `std::thread::sleep(round_delay(retry, round));`, set the countdown for each still-failing key, and clear after re-drive:

```rust
            let delay = round_delay(retry, round);
            for offset in &remaining {
                self.progress.set_retry(
                    reconcile_id,
                    &ops[*offset as usize].key(),
                    delay.as_millis() as u64,
                );
            }
            std::thread::sleep(delay);
            round += 1;
            for offset in remaining {
                let op = &ops[offset as usize];
                self.progress.clear_retry(reconcile_id, &op.key());
                classes[offset as usize] =
                    self.enact_one(reconcile_id, base_ord + offset, op, prior, unit_path, round)?;
            }
```

(Note `remaining` is currently consumed by the `for offset in remaining` loop; capture it once as `let remaining = remaining_ops(ops, &classes);` above the `set_retry` loop, iterate `&remaining` for `set_retry`, then move it into the enact loop. Adjust the earlier `if remaining.is_empty()` check to use the captured binding.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p golemd --lib an_apply_emits_progress_events_for_each_installed_glyph`
Expected: PASS.

- [ ] **Step 5: Run the golemd lib suite**

Run: `cargo test -p golemd --lib`
Expected: PASS — existing tests unaffected (emission is additive).

- [ ] **Step 6: Commit**

```bash
git add apps/golemd/src/foreman.rs
git commit -m "feat(golemd): emit typed progress events from the enact spine

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** the explicit-at-call-site decision (vs a tracing layer) and why; the retry-countdown set/clear lifecycle around the round sleep.

---

### Task 5: golemd — the async HTTP surface (202 apply, GET /reconciles/<id> and /latest, 409 conflict) and migrate the HTTP tests

**Files:**
- Modify: `apps/golemd/src/http.rs` (routes, `apply_manifest`, new `reconcile`/`reconcile_latest`, `ApiError` 409)
- Modify: `apps/golemd/tests/report_api.rs` (migrate the two tests to 202-then-poll / typed errors)
- Create: `apps/golemd/tests/async_apply.rs`
- Test: as above

**Interfaces:**
- Consumes: `Foreman::ingest`, `Foreman::run_reconcile`, `Foreman::progress`, `crate::projection::{project, ReconcileProgress}`, `crate::planroom::PlanRoom` reads via foreman (`latest_attempt`, `wal_steps_for`), and a new `Foreman::attempt_and_steps(id)` read helper + `Foreman::latest_reconcile_id()`.
- Produces (the **cross-crate poll JSON contract** — golemctl and any client depend on exactly this shape):

```json
POST /manifest            (raw manifest bytes, application/octet-stream)
  → 202 { "reconcile_id": 42 }
  → 400-class { "kind": "manifest-undecodable", "message": "…" }
  → 500-class { "kind": "wal-unreadable", "message": "…" }
  → 409       { "kind": "reconcile-in-progress", "message": "…", "reconcile_id": 41 }

GET /reconciles/<id>?after=<seq>     (after optional, default 0)
GET /reconciles/latest?after=<seq>
  → 200 {
      "reconcile_id": 42,
      "phase": "planning|enacting|settling|settled|rolled_back",
      "units": [
        { "unit_path": ["scaly","fishnet-a"],
          "glyphs": [
            { "glyph_key": "apt:podman", "action": "install",
              "state": "pending|in_progress|applied|unchanged|failed|rolled_back",
              "rounds": 1, "next_retry_in_ms": null }
          ] }
      ],
      "events": [
        { "seq": 18, "at": "2026-…Z", "level": "info|warn|error",
          "unit_path": ["scaly","fishnet-a"], "glyph_key": "apt:podman",
          "message": "install apt:podman" }
      ],
      "cursor": 19,
      "report": null | { "revision": …, "outcome": …, "units": … }   // ADR 0029 shape, unchanged
    }
  → 404 { "kind": "not-found", "message": "no reconcile <id>" }   // unknown id / no attempts
```

`cursor` = `max(after, max(events.seq))` so a poll returning no new events echoes the client's `after`. The 202 body's `reconcile_id` is the attempt id `open_attempt` minted (returned before enact begins).

> **Spawn model:** the `apply_manifest` handler calls `foreman.ingest(&bytes)` **synchronously on a blocking task** (it is cheap), and on `Ok((id, sel))` spawns a **second** detached blocking task running `foreman.run_reconcile(id, sel)` (its result is logged via the existing `log_settled`/foreman logging; the handler does not await it). It returns `202 { reconcile_id: id }` immediately. On `Err`, it maps to the typed non-2xx (409 for `ReconcileInProgress`, 400/500 otherwise). The `Arc<Foreman>` is cloned into the spawned task.

- [ ] **Step 1: Write the failing tests** — the async_apply integration test (202 then poll to settle) and the 409 conflict.

Create `apps/golemd/tests/async_apply.rs`:

```rust
use std::sync::Arc;
use std::time::Duration;

use golemd::config::RetryConfig;
use golemd::foreman::Foreman;
use golemd::http;
use golemd::journal::{GlyphOp, Outcome};
use golemd::planroom::MemoryPlanRoom;
use golemd::reconciler::{inverse_of, EnactResult, Reconciler};
use scroll_format::{ContentId, Contents, Glyph, Manifest, Scroll};

struct Ok1;
impl Reconciler for Ok1 {
    fn apply(&self, glyph: &Glyph, cid: ContentId) -> EnactResult<Outcome> {
        Ok(Outcome {
            op: GlyphOp::Install { cid, glyph: glyph.clone() },
            cid,
            inverse: inverse_of(glyph),
            changed: true,
        })
    }
    fn reverse(&self, _o: &Outcome) -> EnactResult<()> {
        Ok(())
    }
}

fn manifest_bytes() -> Vec<u8> {
    let host = Scroll {
        name: "h1".into(),
        policy: None,
        contents: Contents::Glyphs(vec![Glyph::AptPackage { name: "nginx".into() }]),
    };
    scroll_format::to_bytes(&Manifest::from_scrolls(vec![host], "test"))
}

async fn serve(foreman: Foreman) -> String {
    let app = http::router(http::AppState { foreman: Arc::new(foreman) });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn apply_returns_202_then_polls_to_settled_with_report() {
    let foreman = Foreman::new("h1".into(), Box::new(MemoryPlanRoom::new()), Box::new(Ok1))
        .with_retry_config(RetryConfig { max_attempts: 1, base_delay_ms: 0, ..Default::default() });
    let base = serve(foreman).await;

    let resp = reqwest::Client::new()
        .post(format!("{base}/manifest"))
        .header("content-type", "application/octet-stream")
        .body(manifest_bytes())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 202);
    let id = resp.json::<serde_json::Value>().await.unwrap()["reconcile_id"]
        .as_u64()
        .unwrap();

    let mut cursor = 0u64;
    let mut settled = None;
    for _ in 0..50 {
        let p: serde_json::Value = reqwest::get(format!("{base}/reconciles/{id}?after={cursor}"))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        cursor = p["cursor"].as_u64().unwrap();
        if p["phase"] == "settled" {
            settled = Some(p);
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let p = settled.expect("reconcile settled");
    assert_eq!(p["report"]["outcome"], "settled");
    assert_eq!(p["units"][0]["glyphs"][0]["glyph_key"], "apt:nginx");
    assert_eq!(p["units"][0]["glyphs"][0]["state"], "applied");
}

#[tokio::test]
async fn latest_returns_the_most_recent_attempt() {
    let foreman = Foreman::new("h1".into(), Box::new(MemoryPlanRoom::new()), Box::new(Ok1))
        .with_retry_config(RetryConfig { max_attempts: 1, base_delay_ms: 0, ..Default::default() });
    let base = serve(foreman).await;
    reqwest::Client::new()
        .post(format!("{base}/manifest"))
        .header("content-type", "application/octet-stream")
        .body(manifest_bytes())
        .send()
        .await
        .unwrap();
    for _ in 0..50 {
        let p: serde_json::Value = reqwest::get(format!("{base}/reconciles/latest"))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        if p["phase"] == "settled" {
            assert_eq!(p["reconcile_id"], 1);
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("latest never settled");
}
```

Also **migrate** `apps/golemd/tests/report_api.rs`: the first test (`a_failing_reconcile_is_http_200_with_a_rolled_back_report`) becomes a 202-then-poll asserting `report.outcome == "rolled_back"`; the second (`an_undecodable_manifest_is_a_structured_500`) is unchanged in spirit (still a synchronous typed error on the POST). Rewrite the first:

```rust
#[tokio::test]
async fn a_failing_reconcile_settles_rolled_back_via_poll() {
    let foreman = Foreman::new(
        "h1".into(),
        Box::new(MemoryPlanRoom::new()),
        Box::new(FailOne { bad: "apt:bad".into() }),
    )
    .with_retry_config(RetryConfig {
        max_attempts: 1,
        base_delay_ms: 0,
        on_exhaust: OnExhaustConfig::Rollback,
        ..Default::default()
    });
    let base = serve(foreman).await;

    let resp = reqwest::Client::new()
        .post(format!("{base}/manifest"))
        .header("content-type", "application/octet-stream")
        .body(manifest_bytes())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 202);
    let id = resp.json::<serde_json::Value>().await.unwrap()["reconcile_id"].as_u64().unwrap();

    for _ in 0..50 {
        let p: serde_json::Value =
            reqwest::get(format!("{base}/reconciles/{id}")).await.unwrap().json().await.unwrap();
        if !p["report"].is_null() {
            assert_eq!(p["report"]["outcome"], "rolled_back");
            assert_eq!(p["report"]["units"][0]["failures"][0]["class"], "fatal");
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("never settled");
}
```

The undecodable test only changes its expected status if you decide undecodable stays 500 (it does — ADR keeps decode failures as the current typed non-2xx; the ADR calls it "400-class" but the existing code returns 500 for `ManifestUndecodable`, and this plan does **not** change that mapping — recorded as an intentional non-change to avoid touching the error taxonomy beyond the new 409). Keep the assertion `resp.status().as_u16() == 500` and `body["kind"] == "manifest-undecodable"`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p golemd --test async_apply`
Expected: FAIL — routes/handlers do not exist; 202 not returned.

- [ ] **Step 3: Implement the async handlers and routes**

First add read helpers to `Foreman` (`apps/golemd/src/foreman.rs`):

```rust
    pub fn latest_reconcile_id(&self) -> Result<Option<u64>> {
        Ok(self.planroom.latest_attempt()?.map(|a| a.reconcile_id))
    }

    pub fn progress_projection(
        &self,
        reconcile_id: u64,
        after: u64,
    ) -> Result<Option<crate::projection::ReconcileProgress>> {
        let Some(attempt) = self
            .planroom
            .attempts()?
            .into_iter()
            .find(|a| a.reconcile_id == reconcile_id)
        else {
            return Ok(None);
        };
        let steps = self.planroom.wal_steps_for(reconcile_id)?;
        let events = self.progress.events_after(reconcile_id, after);
        let retries = self.progress.retries(reconcile_id);
        let report = if attempt.phase.is_settled() {
            self.settled_report(&attempt, &steps)?
        } else {
            None
        };
        let mut proj =
            crate::projection::project(&attempt, &steps, events, report, &retries);
        proj.cursor = proj.cursor.max(after);
        Ok(Some(proj))
    }
```

`settled_report` rebuilds the `ReconcileReport` for a settled attempt from its revision + WAL. The revision projection already exists (`self.revision(id)`); the per-unit reports are re-derived from the settled steps. **Simplest correct implementation:** cache the report the reconcile produced. Give `Foreman` a `Mutex<BTreeMap<u64, ReconcileReport>>` field `reports`, have `run_reconcile` insert its `report` under `reconcile_id` before returning, and `settled_report` reads it:

```rust
    fn settled_report(
        &self,
        attempt: &ReconcileAttempt,
        _steps: &[WalStep],
    ) -> Result<Option<ReconcileReport>> {
        Ok(self.reports.lock().unwrap().get(&attempt.reconcile_id).cloned())
    }
```

Add `reports: Mutex<std::collections::BTreeMap<u64, ReconcileReport>>` to the struct, init `reports: Mutex::new(std::collections::BTreeMap::new())`, and in `run_reconcile` before `Ok(report)`:

```rust
        self.reports
            .lock()
            .unwrap()
            .insert(reconcile_id, report.clone());
```

(This keeps the report in memory for the reattach window alongside the event ring. It is lost on restart — same honesty as the ring — but on restart recovery re-drives and a fresh poll reports the recovered outcome, which is what a reattaching client wants, per the ADR. Recorded.)

Now rewrite `apps/golemd/src/http.rs`. Routes:

```rust
pub fn router(app: AppState) -> Router {
    Router::new()
        .route("/manifest", post(apply_manifest))
        .route("/reconciles/latest", get(reconcile_latest))
        .route("/reconciles/:id", get(reconcile))
        .route("/state", get(state))
        .route("/revisions", get(revisions))
        .route("/revisions/:id", get(revision))
        .route("/status", get(status))
        .with_state(app)
}
```

(Register `/reconciles/latest` **before** `/reconciles/:id` so `latest` is not captured as an id.)

The async apply handler:

```rust
#[derive(Serialize)]
struct Accepted {
    reconcile_id: u64,
}

async fn apply_manifest(
    AxState(s): AxState<AppState>,
    body: Bytes,
) -> Result<impl IntoResponse, ApiError> {
    let bytes = body.to_vec();
    let foreman = s.foreman.clone();
    let ingested = tokio::task::spawn_blocking(move || foreman.ingest(&bytes))
        .await
        .map_err(|e| ApiError::internal(anyhow::anyhow!("task join: {e}")))?
        .map_err(ApiError::from_foreman)?;
    let (reconcile_id, selected) = ingested;
    let foreman = s.foreman.clone();
    tokio::task::spawn_blocking(move || {
        if let Err(e) = foreman.run_reconcile(reconcile_id, selected) {
            tracing::error!(reconcile_id, error = %e, "reconcile run failed");
        }
    });
    Ok((StatusCode::ACCEPTED, Json(Accepted { reconcile_id })))
}
```

The poll handlers:

```rust
#[derive(serde::Deserialize)]
struct After {
    after: Option<u64>,
}

async fn reconcile(
    AxState(s): AxState<AppState>,
    Path(id): Path<u64>,
    axum::extract::Query(q): axum::extract::Query<After>,
) -> Result<impl IntoResponse, ApiError> {
    let after = q.after.unwrap_or(0);
    match blocking(s.foreman.clone(), move |f| f.progress_projection(id, after)).await? {
        Some(p) => Ok(Json(p)),
        None => Err(ApiError::not_found(format!("no reconcile {id}"))),
    }
}

async fn reconcile_latest(
    AxState(s): AxState<AppState>,
    axum::extract::Query(q): axum::extract::Query<After>,
) -> Result<impl IntoResponse, ApiError> {
    let after = q.after.unwrap_or(0);
    let latest = blocking(s.foreman.clone(), |f| f.latest_reconcile_id()).await?;
    let Some(id) = latest else {
        return Err(ApiError::not_found("no reconcile attempts yet".into()));
    };
    match blocking(s.foreman.clone(), move |f| f.progress_projection(id, after)).await? {
        Some(p) => Ok(Json(p)),
        None => Err(ApiError::not_found(format!("no reconcile {id}"))),
    }
}
```

Update `ApiError::from_foreman` to map the new variant to 409 and carry the id:

```rust
    fn from_foreman(e: crate::foreman::ForemanError) -> Self {
        use crate::foreman::ForemanError::*;
        let status = match e {
            WalUnreadable { .. } | ManifestUndecodable { .. } | Internal(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            ReconcileInProgress { .. } => StatusCode::CONFLICT,
        };
        ApiError {
            status,
            kind: e.kind().to_string(),
            message: e.message(),
        }
    }
```

(The 409 body carries `kind`/`message`; the ADR also wants `reconcile_id` in the 409 body. Add an optional field to `ApiError`: `#[serde(skip_serializing_if = "Option::is_none")] reconcile_id: Option<u64>` set from the variant. Populate it in `from_foreman` by matching `ReconcileInProgress { reconcile_id }` before the status match, and default `None` elsewhere. Add `reconcile_id: None` to the `internal`/`not_found` constructors.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p golemd --test async_apply --test report_api`
Expected: PASS.

- [ ] **Step 5: Run the whole golemd suite**

Run: `cargo test -p golemd`
Expected: PASS — all lib + every integration test (`config_propagation`, `wal_*`, `revisions_projection`, `restart_bracket_fold`, `report_api`, `async_apply`). The lib/integration tests that call `foreman.apply_manifest(...)` directly are unaffected (the sync shim remains).

- [ ] **Step 6: Commit**

```bash
git add apps/golemd/src/http.rs apps/golemd/src/foreman.rs apps/golemd/tests/async_apply.rs apps/golemd/tests/report_api.rs
git commit -m "feat(golemd): async 202 apply + GET /reconciles projection with event log; 409 on conflict

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Assertion-update hotspots (golemd tests that assumed sync apply):** only `apps/golemd/tests/report_api.rs` asserted on the HTTP apply *response body* — both its tests are migrated here. Every other golemd test (`wal_recovery`, `wal_replace_and_fold`, `config_propagation`, `revisions_projection`, `restart_bracket_fold`, and the `foreman`/`projection`/`progress` `--lib` tests) drives `foreman.apply_manifest(...)` **directly in-process** and is unaffected by the transport change because the sync shim preserves its return value.

**Doc backlog:** the two-request protocol contract on `http.rs`; the spawn-and-return model; why `/reconciles/latest` is registered before `/:id`; the in-memory report cache and its restart semantics; the 409 body carrying the pollable id.

---

### Task 6: golemctl — add the iocraft dependency and the typed poll client

**Files:**
- Modify: `apps/golemctl/Cargo.toml`
- Modify: root `Cargo.toml` (add `iocraft` to `[workspace.dependencies]`)
- Create: `apps/golemctl/src/poll.rs`
- Modify: `apps/golemctl/src/main.rs` (add `mod poll;` — main stays as-is otherwise for now)
- Test: `apps/golemctl/src/poll.rs` (`#[cfg(test)] mod tests` — serde round-trip only; the HTTP calls are smoke-tested in Task 9)

**Interfaces:**
- Consumes: the poll JSON contract from Task 5.
- Produces:
  - `pub struct Reconcile202 { pub reconcile_id: u64 }`.
  - `pub enum GlyphState { Pending, InProgress, Applied, Unchanged, Failed, RolledBack }` `#[serde(rename_all = "snake_case")]`.
  - `pub enum Phase { Planning, Enacting, Settling, Settled, RolledBack }` `#[serde(rename_all = "snake_case")]` with `pub fn is_terminal(&self) -> bool` (`Settled | RolledBack`).
  - `pub struct GlyphProgress { pub glyph_key: String, pub action: String, pub state: GlyphState, pub rounds: u32, pub next_retry_in_ms: Option<u64> }`.
  - `pub struct UnitProgress { pub unit_path: Vec<String>, pub glyphs: Vec<GlyphProgress> }`.
  - `pub struct Event { pub seq: u64, pub level: String, pub unit_path: Vec<String>, pub glyph_key: String, pub message: String }` (drop `at` unless rendered — keep it as `pub at: String` for the plain fallback timestamp).
  - `pub struct Progress { pub reconcile_id: u64, pub phase: Phase, pub units: Vec<UnitProgress>, pub events: Vec<Event>, pub cursor: u64, pub report: Option<serde_json::Value> }` (the report is passed through as opaque JSON and pretty-printed by the existing printer).
  - `pub async fn post_manifest(addr: &str, bytes: Vec<u8>) -> anyhow::Result<Reconcile202>` — POSTs, expects 202, decodes; on non-2xx `bail!`s with the typed `{ kind, message }` body (so a 409 prints its message).
  - `pub async fn get_progress(addr: &str, id: u64, after: u64) -> anyhow::Result<Progress>`.

- [ ] **Step 1: Write the failing test** — the progress JSON round-trips into the typed structs.

Create `apps/golemctl/src/poll.rs` with the types (below) and this test:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_progress_payload_deserializes() {
        let json = serde_json::json!({
            "reconcile_id": 42,
            "phase": "enacting",
            "units": [
                { "unit_path": ["scaly","a"],
                  "glyphs": [
                    { "glyph_key": "apt:podman", "action": "install",
                      "state": "in_progress", "rounds": 1, "next_retry_in_ms": null }
                  ] }
            ],
            "events": [
                { "seq": 18, "at": "2026-07-26T00:00:00Z", "level": "info",
                  "unit_path": ["scaly","a"], "glyph_key": "apt:podman",
                  "message": "install apt:podman" }
            ],
            "cursor": 18,
            "report": null
        });
        let p: Progress = serde_json::from_value(json).unwrap();
        assert_eq!(p.reconcile_id, 42);
        assert!(matches!(p.phase, Phase::Enacting));
        assert!(!p.phase.is_terminal());
        assert_eq!(p.units[0].glyphs[0].glyph_key, "apt:podman");
        assert!(matches!(p.units[0].glyphs[0].state, GlyphState::InProgress));
        assert_eq!(p.cursor, 18);
        assert!(p.report.is_none());
    }

    #[test]
    fn a_settled_phase_is_terminal() {
        assert!(Phase::Settled.is_terminal());
        assert!(Phase::RolledBack.is_terminal());
        assert!(!Phase::Planning.is_terminal());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p golemctl poll::`
Expected: FAIL — `poll` module and types do not exist.

- [ ] **Step 3: Add the dependency and implement the client**

In root `Cargo.toml` `[workspace.dependencies]` add:

```toml
iocraft      = { version = "=0.8.2", features = ["unstable-output-streams"] }
```

In `apps/golemctl/Cargo.toml` `[dependencies]` add:

```toml
iocraft      = { workspace = true }
```

> **If `cargo build -p golemctl` fails to resolve the stderr render API on released 0.8.2**, add to the root `Cargo.toml`:
> ```toml
> [patch.crates-io]
> iocraft = { git = "https://github.com/ccbrown/iocraft", branch = "main" }
> ```
> This is the same patch the devenv workspace uses; record which path was taken in the commit body.

Write `apps/golemctl/src/poll.rs`:

```rust
use anyhow::{bail, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Reconcile202 {
    pub reconcile_id: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GlyphState {
    Pending,
    InProgress,
    Applied,
    Unchanged,
    Failed,
    RolledBack,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Planning,
    Enacting,
    Settling,
    Settled,
    RolledBack,
}

impl Phase {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Phase::Settled | Phase::RolledBack)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct GlyphProgress {
    pub glyph_key: String,
    pub action: String,
    pub state: GlyphState,
    pub rounds: u32,
    pub next_retry_in_ms: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UnitProgress {
    pub unit_path: Vec<String>,
    pub glyphs: Vec<GlyphProgress>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Event {
    pub seq: u64,
    pub at: String,
    pub level: String,
    pub unit_path: Vec<String>,
    pub glyph_key: String,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Progress {
    pub reconcile_id: u64,
    pub phase: Phase,
    pub units: Vec<UnitProgress>,
    pub events: Vec<Event>,
    pub cursor: u64,
    pub report: Option<serde_json::Value>,
}

pub async fn post_manifest(addr: &str, bytes: Vec<u8>) -> Result<Reconcile202> {
    let url = format!("{}/manifest", addr.trim_end_matches('/'));
    let resp = reqwest::Client::new()
        .post(&url)
        .header("content-type", "application/octet-stream")
        .body(bytes)
        .send()
        .await?;
    let status = resp.status();
    let text = resp.text().await?;
    if status.as_u16() != 202 {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(msg) = v.get("message").and_then(|m| m.as_str()) {
                bail!("{status}: {msg}");
            }
        }
        bail!("{status}: {text}");
    }
    Ok(serde_json::from_str(&text)?)
}

pub async fn get_progress(addr: &str, id: u64, after: u64) -> Result<Progress> {
    let url = format!("{}/reconciles/{id}?after={after}", addr.trim_end_matches('/'));
    let resp = reqwest::get(&url).await?;
    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        bail!("{status}: {text}");
    }
    Ok(serde_json::from_str(&text)?)
}
```

Add `mod poll;` to `apps/golemctl/src/main.rs` (top, near the other items).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p golemctl poll::`
Expected: PASS.

- [ ] **Step 5: Confirm the crate still builds with iocraft linked**

Run: `cargo build -p golemctl`
Expected: builds. If the stderr render API is missing, apply the git patch (above), rebuild, and note it in the commit.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml apps/golemctl/Cargo.toml apps/golemctl/src/poll.rs apps/golemctl/src/main.rs
git commit -m "feat(golemctl): add iocraft dep and the typed fire-then-poll client

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** the poll client's error mapping (typed `message` surfaced to the user, esp. the 409); the iocraft pin decision and whether the git patch was needed.

---

### Task 7: golemctl — the TUI model and its event fold (devenv-tui pattern, unit tree)

**Files:**
- Create: `apps/golemctl/src/model.rs`
- Modify: `apps/golemctl/src/main.rs` (add `mod model;`)
- Test: `apps/golemctl/tests/model_tests.rs` (new integration test target) — the devenv `tui_tests.rs` model-level pattern.

**Interfaces:**
- Consumes: `crate::poll::{Progress, Phase, GlyphState, UnitProgress, GlyphProgress, Event}`.
- Produces:
  - `pub enum UnitState { Active, Settled, Failed }` and `pub struct GlyphRow { pub glyph_key: String, pub action: String, pub state: GlyphState, pub rounds: u32, pub next_retry_in_ms: Option<u64> }`.
  - `pub struct UnitNode { pub unit_path: Vec<String>, pub glyphs: Vec<GlyphRow>, pub logs: std::collections::VecDeque<String>, pub state: UnitState }`.
  - `pub struct ApplyModel { pub reconcile_id: u64, pub phase: Phase, pub units: Vec<UnitNode>, pub cursor: u64, pub report: Option<serde_json::Value> }` with:
    - `pub fn new() -> Self`
    - `pub fn apply_progress(&mut self, p: Progress)` — upserts each unit by `unit_path`, replaces its glyph rows, appends `p.events` (formatted `"<glyph_key>: <message>"`) to the matching unit node's `logs` ring (cap `LOG_RING_CAP = 200`), advances `cursor`, sets `phase`/`report`, and recomputes each unit's `UnitState` (any glyph `Failed` → `Failed`; all `Applied`/`Unchanged`/`RolledBack` → `Settled`; else `Active`).
    - `pub fn is_settled(&self) -> bool` (`self.phase.is_terminal()`).
  - `pub const LOG_RING_CAP: usize = 200;`.

> **devenv mapping (recorded):** devenv's `ActivityModel` keys `Activity` nodes by u64 id with a `parent_id` and a per-node `VecDeque` log ring, and folds `ActivityEvent`s (`create_activity`/`handle_activity_complete`/`handle_activity_log`) into it. golem's model keys `UnitNode`s by `unit_path` (the projection's `units` array *is* the tree), and `apply_progress` is the single fold replacing devenv's per-event handlers — one poll response carries the whole unit set plus the new event slice, so the fold is an upsert-and-append rather than three separate handlers. `UnitState`↔devenv's `NixActivityState` (`Active`/`Completed{success}`); a glyph's `GlyphState`↔the per-line status glyph.

- [ ] **Step 1: Write the failing test** — applying two polls upserts units, appends logs, and settles.

Create `apps/golemctl/tests/model_tests.rs`:

```rust
use golemctl::model::{ApplyModel, UnitState};
use golemctl::poll::{Event, GlyphProgress, GlyphState, Phase, Progress, UnitProgress};

fn glyph(key: &str, state: GlyphState) -> GlyphProgress {
    GlyphProgress { glyph_key: key.into(), action: "install".into(), state, rounds: 1, next_retry_in_ms: None }
}

fn event(seq: u64, unit: &[&str], key: &str, msg: &str) -> Event {
    Event {
        seq,
        at: "2026-07-26T00:00:00Z".into(),
        level: "info".into(),
        unit_path: unit.iter().map(|s| s.to_string()).collect(),
        glyph_key: key.into(),
        message: msg.into(),
    }
}

#[test]
fn applying_progress_builds_the_unit_tree_and_appends_logs() {
    let mut m = ApplyModel::new();
    m.apply_progress(Progress {
        reconcile_id: 1,
        phase: Phase::Enacting,
        units: vec![UnitProgress {
            unit_path: vec!["scaly".into(), "a".into()],
            glyphs: vec![glyph("apt:podman", GlyphState::InProgress)],
        }],
        events: vec![event(1, &["scaly", "a"], "apt:podman", "install apt:podman")],
        cursor: 1,
        report: None,
    });
    assert_eq!(m.units.len(), 1);
    assert_eq!(m.units[0].unit_path, vec!["scaly", "a"]);
    assert!(matches!(m.units[0].state, UnitState::Active));
    assert_eq!(m.units[0].logs.len(), 1);
    assert!(m.units[0].logs[0].contains("install apt:podman"));
    assert_eq!(m.cursor, 1);

    m.apply_progress(Progress {
        reconcile_id: 1,
        phase: Phase::Settled,
        units: vec![UnitProgress {
            unit_path: vec!["scaly".into(), "a".into()],
            glyphs: vec![glyph("apt:podman", GlyphState::Applied)],
        }],
        events: vec![event(2, &["scaly", "a"], "apt:podman", "apt:podman done")],
        cursor: 2,
        report: Some(serde_json::json!({ "outcome": "settled" })),
    });
    assert_eq!(m.units.len(), 1);
    assert!(matches!(m.units[0].state, UnitState::Settled));
    assert_eq!(m.units[0].logs.len(), 2);
    assert!(m.is_settled());
    assert!(m.report.is_some());
}
```

This requires golemctl to expose a library surface. Add `apps/golemctl/src/lib.rs` re-exporting the modules (`pub mod poll; pub mod model; pub mod view;`) and have `main.rs` `use golemctl::{...}`. (golemctl is currently a bin-only crate; add a lib target. In `Cargo.toml` no change is needed — a `src/lib.rs` beside `src/main.rs` is auto-detected as the lib; `main.rs` then does `use golemctl::…`.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p golemctl --test model_tests`
Expected: FAIL — no lib target / `model` module.

- [ ] **Step 3: Implement the lib target and the model**

Create `apps/golemctl/src/lib.rs`:

```rust
pub mod model;
pub mod poll;
pub mod view;
```

(Task 8 fills `view`; add `pub mod view;` now and create an empty `apps/golemctl/src/view.rs` with just `use` placeholders so the lib compiles — or land Task 8's view first. Simplest: create `view.rs` containing only what compiles, filled in Task 8. To keep this task green, put a minimal `pub fn placeholder() {}` in `view.rs` and remove it in Task 8.)

Write `apps/golemctl/src/model.rs`:

```rust
use std::collections::VecDeque;

use crate::poll::{GlyphState, Phase, Progress};

pub const LOG_RING_CAP: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnitState {
    Active,
    Settled,
    Failed,
}

#[derive(Debug, Clone)]
pub struct GlyphRow {
    pub glyph_key: String,
    pub action: String,
    pub state: GlyphState,
    pub rounds: u32,
    pub next_retry_in_ms: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct UnitNode {
    pub unit_path: Vec<String>,
    pub glyphs: Vec<GlyphRow>,
    pub logs: VecDeque<String>,
    pub state: UnitState,
}

#[derive(Debug, Clone)]
pub struct ApplyModel {
    pub reconcile_id: u64,
    pub phase: Phase,
    pub units: Vec<UnitNode>,
    pub cursor: u64,
    pub report: Option<serde_json::Value>,
}

fn unit_state(glyphs: &[GlyphRow]) -> UnitState {
    if glyphs.iter().any(|g| g.state == GlyphState::Failed) {
        UnitState::Failed
    } else if glyphs.iter().all(|g| {
        matches!(
            g.state,
            GlyphState::Applied | GlyphState::Unchanged | GlyphState::RolledBack
        )
    }) {
        UnitState::Settled
    } else {
        UnitState::Active
    }
}

impl ApplyModel {
    pub fn new() -> Self {
        Self {
            reconcile_id: 0,
            phase: Phase::Planning,
            units: Vec::new(),
            cursor: 0,
            report: None,
        }
    }

    pub fn apply_progress(&mut self, p: Progress) {
        self.reconcile_id = p.reconcile_id;
        self.phase = p.phase;
        self.cursor = p.cursor;
        self.report = p.report;
        for up in p.units {
            let glyphs: Vec<GlyphRow> = up
                .glyphs
                .into_iter()
                .map(|g| GlyphRow {
                    glyph_key: g.glyph_key,
                    action: g.action,
                    state: g.state,
                    rounds: g.rounds,
                    next_retry_in_ms: g.next_retry_in_ms,
                })
                .collect();
            let state = unit_state(&glyphs);
            match self.units.iter_mut().find(|u| u.unit_path == up.unit_path) {
                Some(node) => {
                    node.glyphs = glyphs;
                    node.state = state;
                }
                None => self.units.push(UnitNode {
                    unit_path: up.unit_path,
                    glyphs,
                    logs: VecDeque::new(),
                    state,
                }),
            }
        }
        for ev in p.events {
            if let Some(node) = self.units.iter_mut().find(|u| u.unit_path == ev.unit_path) {
                node.logs.push_back(format!("{}: {}", ev.glyph_key, ev.message));
                while node.logs.len() > LOG_RING_CAP {
                    node.logs.pop_front();
                }
            }
        }
    }

    pub fn is_settled(&self) -> bool {
        self.phase.is_terminal()
    }
}

impl Default for ApplyModel {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p golemctl --test model_tests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/golemctl/src/lib.rs apps/golemctl/src/model.rs apps/golemctl/src/view.rs apps/golemctl/src/main.rs apps/golemctl/tests/model_tests.rs
git commit -m "feat(golemctl): apply model + progress fold (unit tree, per-unit log ring)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** the devenv-tui mapping (unit_path-keyed tree vs id-keyed activities; one fold vs three handlers); `UnitState` derivation; the log-ring cap.

---

### Task 8: golemctl — the pure iocraft view + Spinner/StatusIndicator components (view logic tested at model level)

**Files:**
- Modify: `apps/golemctl/src/view.rs` (replace the placeholder)
- Test: `apps/golemctl/tests/model_tests.rs` (add view-render string assertions — the devenv `render_to_string` pattern)

**Interfaces:**
- Consumes: `crate::model::{ApplyModel, UnitNode, GlyphRow, UnitState}`, `crate::poll::GlyphState`, `iocraft::prelude::*`.
- Produces:
  - `pub const CHECKMARK: &str = "✓"; pub const XMARK: &str = "✗"; pub const ROLLED_BACK: &str = "↩"; pub const UNCHANGED: &str = "·"; pub const SPINNER_FRAMES: &[&str] = &["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"];`.
  - `pub fn glyph_mark(state: GlyphState) -> &'static str` — pure: `Applied→✓`, `Unchanged→·`, `RolledBack→↩`, `Failed→✗`, `InProgress`/`Pending`→`SPINNER_FRAMES[0]` (the static frame the pure renderer emits; the live `Spinner` component animates it).
  - `pub fn unit_mark(state: UnitState) -> &'static str` — `Settled→✓`, `Failed→✗`, `Active→SPINNER_FRAMES[0]`.
  - `pub fn render_to_string(model: &ApplyModel, width: usize) -> String` — builds the element tree and renders it to a plain string (devenv's `element.render(Some(width)).to_string()`), the **testable** pure view. Used by tests and as the non-animated frame source.
  - `#[component] pub fn Spinner(...)` and `#[component] pub fn StatusIndicator(...)` — the animated components (used only by the live runtime in Task 9; smoke-tested there).
  - `pub fn view(model: &ApplyModel) -> impl Into<AnyElement<'static>>` — the pure element tree: one row per unit (`unit_mark` + `" / ".join(unit_path)`), each unit's glyph rows beneath (`glyph_mark` + `glyph_key` + optional `next_retry_in_ms` countdown when `Some`), and under an `Active` unit its recent `logs` lines (last N, like devenv's `ChildActivityLimit`).

- [ ] **Step 1: Write the failing test** — the rendered string shows unit paths, marks, and active-unit logs.

Add to `apps/golemctl/tests/model_tests.rs`:

```rust
use golemctl::view;

#[test]
fn the_view_renders_unit_paths_marks_and_active_logs() {
    let mut m = ApplyModel::new();
    m.apply_progress(Progress {
        reconcile_id: 1,
        phase: Phase::Enacting,
        units: vec![
            UnitProgress {
                unit_path: vec!["scaly".into(), "base".into()],
                glyphs: vec![glyph("apt:htop", GlyphState::Applied)],
            },
            UnitProgress {
                unit_path: vec!["scaly".into(), "fishnet-a".into()],
                glyphs: vec![glyph("apt:podman", GlyphState::InProgress)],
            },
        ],
        events: vec![event(1, &["scaly", "fishnet-a"], "apt:podman", "install apt:podman")],
        cursor: 1,
        report: None,
    });
    let out = view::render_to_string(&m, 100);
    assert!(out.contains("scaly / base"));
    assert!(out.contains("scaly / fishnet-a"));
    assert!(out.contains("apt:htop"));
    assert!(out.contains(view::CHECKMARK));
    assert!(out.contains("install apt:podman"));
}

#[test]
fn a_failed_glyph_shows_the_x_mark() {
    let mut m = ApplyModel::new();
    m.apply_progress(Progress {
        reconcile_id: 1,
        phase: Phase::RolledBack,
        units: vec![UnitProgress {
            unit_path: vec!["scaly".into(), "canary".into()],
            glyphs: vec![glyph("systemd:canary.service", GlyphState::Failed)],
        }],
        events: vec![],
        cursor: 0,
        report: Some(serde_json::json!({ "outcome": "partial" })),
    });
    let out = view::render_to_string(&m, 100);
    assert!(out.contains(view::XMARK));
    assert!(out.contains("scaly / canary"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p golemctl --test model_tests the_view_renders`
Expected: FAIL — `view::render_to_string`/`view` not implemented (placeholder only).

- [ ] **Step 3: Implement the view and components**

Replace `apps/golemctl/src/view.rs`:

```rust
use iocraft::prelude::*;

use crate::model::{ApplyModel, UnitState};
use crate::poll::GlyphState;

pub const CHECKMARK: &str = "✓";
pub const XMARK: &str = "✗";
pub const ROLLED_BACK: &str = "↩";
pub const UNCHANGED: &str = "·";
pub const SPINNER_FRAMES: &[&str] = &[
    "⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏",
];
const ACTIVE_LOG_LINES: usize = 5;

pub fn glyph_mark(state: GlyphState) -> &'static str {
    match state {
        GlyphState::Applied => CHECKMARK,
        GlyphState::Unchanged => UNCHANGED,
        GlyphState::RolledBack => ROLLED_BACK,
        GlyphState::Failed => XMARK,
        GlyphState::Pending | GlyphState::InProgress => SPINNER_FRAMES[0],
    }
}

pub fn unit_mark(state: UnitState) -> &'static str {
    match state {
        UnitState::Settled => CHECKMARK,
        UnitState::Failed => XMARK,
        UnitState::Active => SPINNER_FRAMES[0],
    }
}

pub fn view(model: &ApplyModel) -> impl Into<AnyElement<'static>> {
    let mut rows: Vec<AnyElement<'static>> = Vec::new();
    for unit in &model.units {
        let path = unit.unit_path.join(" / ");
        let header = format!("{} {}", unit_mark(unit.state), path);
        rows.push(element!(Text(content: header)).into_any());
        for g in &unit.glyphs {
            let mut line = format!("  {} {}", glyph_mark(g.state), g.glyph_key);
            if let Some(ms) = g.next_retry_in_ms {
                line.push_str(&format!("  (retry in {}ms)", ms));
            }
            rows.push(element!(Text(content: line)).into_any());
        }
        if unit.state == UnitState::Active {
            let start = unit.logs.len().saturating_sub(ACTIVE_LOG_LINES);
            for log in unit.logs.iter().skip(start) {
                rows.push(element!(Text(content: format!("    {log}"))).into_any());
            }
        }
    }
    element! {
        View(flex_direction: FlexDirection::Column) {
            #(rows)
        }
    }
}

pub fn render_to_string(model: &ApplyModel, width: usize) -> String {
    let mut element: AnyElement<'static> = view(model).into();
    element.render(Some(width)).to_string()
}

#[component]
pub fn Spinner(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut frame = hooks.use_state(|| 0usize);
    hooks.use_future(async move {
        loop {
            smol::Timer::after(std::time::Duration::from_millis(80)).await;
            let Some(v) = frame.try_get() else { break };
            frame.set((v + 1) % SPINNER_FRAMES.len());
        }
    });
    element!(Text(content: SPINNER_FRAMES[frame.get()]))
}
```

> **iocraft API note (verify at build-out):** devenv's `Spinner` uses `hooks.use_state`/`hooks.use_future` and `tokio::time::sleep`. iocraft's async runtime is `smol`-based by default; devenv wires tokio via a feature. golemctl already runs on tokio (`#[tokio::main]`). Use whichever timer iocraft's `use_future` expects on 0.8.2 — `tokio::time::sleep` if the tokio feature is on, else `smol::Timer`. The `render_to_string` API (`element.render(Some(width)).to_string()`) is the devenv test helper; confirm the exact method name on 0.8.2 (`render`/`to_string`) and adjust. If the pure `render` helper differs, the **model-level string test still holds** by rendering the same element tree — adjust only the call, never the assertions.

Remove the `placeholder()` fn.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p golemctl --test model_tests`
Expected: PASS (all model + view-render tests).

- [ ] **Step 5: Commit**

```bash
git add apps/golemctl/src/view.rs apps/golemctl/tests/model_tests.rs
git commit -m "feat(golemctl): pure iocraft view + spinner/status marks (model-level tested)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** which view logic is pure/tested vs which is the animated runtime (smoke-only); the per-glyph mark vocabulary (`✓ · ↩ ✗` + spinner) mapping to `GlyphState`; the active-unit log window (`ACTIVE_LOG_LINES`, devenv's `ChildActivityLimit` analogue); the iocraft timer/render-API caveat and what was chosen.

---

### Task 9: golemctl — wire apply: fire-then-poll, live TUI on a TTY, plain/`--json` fallback, print report on settle

**Files:**
- Create: `apps/golemctl/src/apply.rs`
- Modify: `apps/golemctl/src/main.rs` (clap: `apply` gains `--json`/`--reattach`; call `apply::run`)
- Modify: `apps/golemctl/src/lib.rs` (`pub mod apply;`)
- Test: `apps/golemctl/src/apply.rs` (`#[cfg(test)]` — pure helpers only: the plain-line formatter and the poll-loop termination predicate; the live iocraft runtime is smoke-tested by hand, stated honestly)

**Interfaces:**
- Consumes: `crate::poll::{post_manifest, get_progress, Progress}`, `crate::model::ApplyModel`, `crate::view`, and the existing report pretty-printer (currently the `print_response` JSON pretty-print in `main.rs` — extract it to `pub fn print_report(report: &serde_json::Value)`).
- Produces:
  - `pub async fn run(source: &Path, addr: &str, json: bool, reattach: bool) -> Result<()>` — the apply entry the CLI calls.
  - `pub fn plain_line(ev: &crate::poll::Event) -> String` — the non-TTY line format (`"[level] unit / path  glyph_key: message"`), pure and tested.
  - `pub fn should_stop(p: &Progress) -> bool` — `p.phase.is_terminal()`, pure and tested (one final read then stop, per the ADR's terminal-backoff note).

> **Control flow (recorded):** `run` compiles the manifest (reuse the existing `manifest_bytes`/`compile_emet` in `main.rs` — move them into `apply.rs` or call them), `post_manifest` to get the id (unless `--reattach`, which starts from `get_progress(addr, latest, 0)` via `/reconciles/latest` — add a `get_latest` to `poll.rs` mirroring `get_progress`). Then: if `!json && stdout_is_tty()` → drive the **live iocraft loop** (Task 8's components), polling every ~1s, applying each `Progress` into the `ApplyModel`, re-rendering, until `should_stop`; else → the **plain loop**: poll every ~1s, print each new `events` line via `plain_line`, until `should_stop`. Either way, on settle print the final `report` (`print_report`, or raw JSON if `--json`). TTY detection: `std::io::IsTerminal` on stdout (std, no dep).

> **Honest testing boundary:** `plain_line` and `should_stop` are unit-tested. The live iocraft render loop (spinner animation, redraw throttling, stderr rendering) is **not** automatically tested — it is verified in the Task 11 live smoke run. This is stated because terminal rendering cannot be meaningfully asserted in CI without a pty harness this plan does not build.

- [ ] **Step 1: Write the failing test** — `plain_line` formats an event and `should_stop` fires only on terminal phases.

Add to `apps/golemctl/src/apply.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::poll::{Event, Phase, Progress};

    fn ev() -> Event {
        Event {
            seq: 1,
            at: "2026-07-26T00:00:00Z".into(),
            level: "warn".into(),
            unit_path: vec!["scaly".into(), "canary".into()],
            glyph_key: "systemd:canary.service".into(),
            message: "enact failed (round 1)".into(),
        }
    }

    fn prog(phase: Phase) -> Progress {
        Progress { reconcile_id: 1, phase, units: vec![], events: vec![], cursor: 0, report: None }
    }

    #[test]
    fn plain_line_names_level_unit_and_message() {
        let line = plain_line(&ev());
        assert!(line.contains("warn"));
        assert!(line.contains("scaly / canary"));
        assert!(line.contains("systemd:canary.service"));
        assert!(line.contains("enact failed"));
    }

    #[test]
    fn should_stop_only_on_terminal_phase() {
        assert!(!should_stop(&prog(Phase::Enacting)));
        assert!(should_stop(&prog(Phase::Settled)));
        assert!(should_stop(&prog(Phase::RolledBack)));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p golemctl --lib apply::`
Expected: FAIL — `apply` module missing.

- [ ] **Step 3: Implement apply**

Add `pub mod apply;` to `apps/golemctl/src/lib.rs`. Create `apps/golemctl/src/apply.rs`:

```rust
use std::io::IsTerminal;
use std::path::Path;
use std::time::Duration;

use anyhow::Result;

use crate::model::ApplyModel;
use crate::poll::{get_progress, post_manifest, Event, Progress};
use crate::view;

pub fn plain_line(ev: &Event) -> String {
    format!(
        "[{}] {}  {}: {}",
        ev.level,
        ev.unit_path.join(" / "),
        ev.glyph_key,
        ev.message
    )
}

pub fn should_stop(p: &Progress) -> bool {
    p.phase.is_terminal()
}

pub fn print_report(report: &serde_json::Value) {
    match serde_json::to_string_pretty(report) {
        Ok(s) => println!("{s}"),
        Err(_) => println!("{report}"),
    }
}

pub async fn run(bytes: Vec<u8>, addr: &str, json: bool) -> Result<()> {
    let accepted = post_manifest(addr, bytes).await?;
    let id = accepted.reconcile_id;
    if !json && std::io::stdout().is_terminal() {
        run_tui(addr, id).await
    } else {
        run_plain(addr, id, json).await
    }
}

async fn run_plain(addr: &str, id: u64, json: bool) -> Result<()> {
    let mut cursor = 0u64;
    loop {
        let p = get_progress(addr, id, cursor).await?;
        cursor = p.cursor;
        for ev in &p.events {
            println!("{}", plain_line(ev));
        }
        if should_stop(&p) {
            if let Some(report) = &p.report {
                if json {
                    println!("{report}");
                } else {
                    print_report(report);
                }
            }
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
}

async fn run_tui(addr: &str, id: u64) -> Result<()> {
    let mut model = ApplyModel::new();
    let mut cursor = 0u64;
    loop {
        let p = get_progress(addr, id, cursor).await?;
        cursor = p.cursor;
        let terminal = should_stop(&p);
        let report = p.report.clone();
        model.apply_progress(p);
        eprint!("\x1b[2J\x1b[H");
        eprintln!("{}", view::render_to_string(&model, 100));
        if terminal {
            if let Some(report) = &report {
                print_report(report);
            }
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
}
```

> **Live-runtime build-out note:** `run_tui` above is the **simplest correct** live loop — it re-renders the pure `view` to stderr each poll (clearing with an ANSI home/erase), which gives a live-updating tree without the full iocraft render loop. If the richer animated experience (self-animating `Spinner` between polls) is wanted, replace `run_tui`'s body with iocraft's `element! { … }.render_loop()` driven by a shared `Arc<Mutex<ApplyModel>>` updated from the poll task and a `use_future`/`use_state` redraw signal, mirroring devenv's `throttled_notify_loop` (stderr, no alternate screen). The plan ships the string-redraw loop as the correct baseline; the animated upgrade is optional polish verified in Task 11. Either way `render_to_string`/`view` and the model fold are unchanged — the tested surface is stable.

Now update `apps/golemctl/src/main.rs`: extract manifest compilation, add the flags, and call `apply::run`:

```rust
#[derive(Subcommand, Debug)]
enum Cmd {
    Apply {
        source: PathBuf,
        addr: String,
        #[arg(long)]
        json: bool,
    },
    State { addr: String },
    History { addr: String },
    Show { addr: String, id: u64 },
}
```

In `main`'s match:

```rust
        Cmd::Apply { source, addr, json } => {
            let bytes = manifest_bytes(&source).await?;
            golemctl::apply::run(bytes, &addr, json).await
        }
```

(`manifest_bytes`/`compile_emet` stay in `main.rs`; `apply::run` takes the compiled bytes. `print_response`/`fetch_and_print` for the read subcommands are unchanged.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p golemctl --lib apply::`
Expected: PASS.

- [ ] **Step 5: Build and run the whole golemctl suite**

Run: `cargo test -p golemctl`
Expected: PASS (poll + model + view + apply tests). Then `cargo build -p golemctl` to confirm the bin links.

- [ ] **Step 6: Commit**

```bash
git add apps/golemctl/src/apply.rs apps/golemctl/src/main.rs apps/golemctl/src/lib.rs
git commit -m "feat(golemctl): fire-then-poll apply — live tree on TTY, plain/json fallback

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** the TTY-vs-pipe branch and `--json`; the ~1s poll cadence and one-final-read terminal stop; which surface is tested vs smoke-only (the honest boundary); the string-redraw baseline vs the optional animated iocraft loop.

---

### Task 10: fleet — exec golemctl per host; remove the golemd_client apply path and the ffa1414 stopgap

**Files:**
- Modify: `apps/fleet/deploy.py` (add `resolve_golemctl`)
- Modify: `apps/fleet/cli.py` (`apply` body; delete `_render_apply_transport_error`; retire the `_render_report` apply call)
- Modify: `apps/fleet/golemd_client.py` (delete `apply_manifest`, `_APPLY_TIMEOUT`)
- Modify: `apps/fleet/tests/test_apply_render.py` (migrate the transport/continue tests to the golemctl-exec shape; keep the `_render_report` unit tests — the function stays, only its apply call is removed)

**Interfaces:**
- Consumes: `VmRecord.golemd_port`, `VmRecord.name`, `paths.root`.
- Produces:
  - `deploy_ops.resolve_golemctl(paths) -> Path` — resolution order (DECIDED): `GOLEMCTL_BIN` env var → `paths.root / "target" / "debug" / "golemctl"` (the `cargo build --workspace` / `build` devenv script output) → `shutil.which("golemctl")` → raise `FleetError("golemctl not found; run `build` (cargo build --workspace) or set GOLEMCTL_BIN")`. **No HTTP fallback** — the stopgap and `golemd_client.apply_manifest` are removed outright (ADR §7; the cleaner end-state over keeping a dual path).
  - `apply` execs `[golemctl, "apply", str(manifest_path), f"http://127.0.0.1:{record.golemd_port}"]` per host (plus `--json` when `raw`), streaming golemctl's stdout/stderr straight through (the live TUI renders on the operator's terminal). A non-zero exit is reported per host and the loop continues to the next.

> **golemctl runs locally against the forwarded port** — NOT over SSH. The guest's golemd is reachable at `127.0.0.1:<golemd_port>` on the controller (the qemu hostfwd, `vm.py`), exactly where `golemd_client` posted. fleet passes the already-compiled `manifest.bin` path (golemctl accepts a prebuilt manifest, not only `.emet`).

- [ ] **Step 1: Write the failing test** — the apply command execs golemctl per host and continues past a failing host.

Replace `ApplyTransportErrorTests` in `apps/fleet/tests/test_apply_render.py` with a golemctl-exec test:

```python
class ApplyExecTests(unittest.TestCase):
    def test_apply_execs_golemctl_per_host_and_continues_on_failure(self):
        from fleet import cli
        from fleet.state import VmRecord

        records = [
            VmRecord(name="vm-1", ssh_port=2201, golemd_port=8001, pid=1,
                     disk="/dev/null", pidfile="/dev/null", console_log="/dev/null"),
            VmRecord(name="vm-2", ssh_port=2202, golemd_port=8002, pid=2,
                     disk="/dev/null", pidfile="/dev/null", console_log="/dev/null"),
        ]

        manifest_path = Path("/tmp/fleet-test-manifest.bin")
        manifest_path.write_bytes(b"\x00")
        self.addCleanup(manifest_path.unlink, missing_ok=True)

        calls = []

        def fake_run(argv, **kwargs):
            calls.append(argv)
            rc = 1 if "8001" in " ".join(argv) else 0
            return subprocess.CompletedProcess(argv, rc)

        with (
            mock.patch.object(cli, "_target_records", return_value=records),
            mock.patch.object(cli.deploy_ops, "compile_manifest", return_value=manifest_path),
            mock.patch.object(cli.deploy_ops, "manifest_scroll_names", return_value=[]),
            mock.patch.object(cli.deploy_ops, "resolve_golemctl", return_value=Path("/usr/bin/golemctl")),
            mock.patch("fleet.cli.subprocess.run", side_effect=fake_run),
        ):
            runner = CliRunner()
            result = runner.invoke(cli.app, ["apply", str(manifest_path)])

        self.assertEqual(result.exit_code, 0, result.output)
        self.assertNotIn("Traceback", result.output)
        self.assertEqual(len(calls), 2)
        joined = [" ".join(str(a) for a in argv) for argv in calls]
        self.assertTrue(any("127.0.0.1:8001" in j for j in joined))
        self.assertTrue(any("127.0.0.1:8002" in j for j in joined))
        self.assertTrue(all("golemctl" in j and "apply" in j for j in joined))
        self.assertIn("vm-1", result.output)
```

(Ensure `import subprocess` is present at the top of the test module.)

- [ ] **Step 2: Run test to verify it fails**

Run: `python -m pytest apps/fleet/tests/test_apply_render.py::ApplyExecTests -q`
(from repo root, with `PYTHONPATH=apps`; the devenv `test` script runs `cargo test`, so run pytest directly)
Expected: FAIL — `resolve_golemctl` and the exec body do not exist.

- [ ] **Step 3: Implement resolve_golemctl and the exec apply body**

In `apps/fleet/deploy.py` add (with `import os, shutil` at top if absent):

```python
def resolve_golemctl(paths: Paths) -> Path:
    override = os.environ.get("GOLEMCTL_BIN")
    if override:
        return Path(override)
    built = paths.root / "target" / "debug" / "golemctl"
    if built.exists():
        return built
    found = shutil.which("golemctl")
    if found:
        return Path(found)
    raise FleetError(
        "golemctl not found; run `build` (cargo build --workspace) or set GOLEMCTL_BIN"
    )
```

In `apps/fleet/cli.py`, replace the per-host loop body in `apply` (the `try/except`, the status-code branch, and the `_render_report` call) with a golemctl exec:

```python
    golemctl = deploy_ops.resolve_golemctl(p)
    for record in records:
        console.print(f"[bold]Applying to {record.name}[/bold]…")
        argv = [
            str(golemctl),
            "apply",
            str(manifest_path),
            f"http://127.0.0.1:{record.golemd_port}",
        ]
        if raw:
            argv.append("--json")
        result = subprocess.run(argv, cwd=str(p.root))
        if result.returncode != 0:
            console.print(
                f"  [red]{record.name}: golemctl apply exited {result.returncode}[/red]"
            )
            continue
```

Delete the `_render_apply_transport_error` function entirely. Keep `_render_report`, `_render_apply_error`, `_render_revision`, `_render_glyph_line`, `_render_failure_line` (their unit tests stay green — only the *call* from `apply` is removed; golemctl now prints the report). Ensure `import subprocess` is at the top of `cli.py` (it already imports it for logs). Remove the now-unused `golemd_client` and `httpx` imports from `cli.py` **only if** nothing else in `cli.py` uses them — `state`/`status` may still import `golemd_client`; grep before removing (`grep -n "golemd_client\|httpx" apps/fleet/cli.py`). Keep the imports if `status`/`state` calls remain.

In `apps/fleet/golemd_client.py`, delete `apply_manifest` and `_APPLY_TIMEOUT`; keep `status`, `state`, `_base_url`. Remove the now-unused `VmRecord`/`httpx` bits only if unused after deletion (they are still used by `status`/`state`, so keep them).

- [ ] **Step 4: Run test to verify it passes**

Run: `python -m pytest apps/fleet/tests/test_apply_render.py -q`
Expected: PASS — the new `ApplyExecTests` and the retained `RenderReportTests` (`_render_report` unit tests) all green.

- [ ] **Step 5: Confirm no dangling references to the removed symbols**

Run: `grep -rn "apply_manifest\|_APPLY_TIMEOUT\|_render_apply_transport_error" apps/fleet`
Expected: no matches in `cli.py`/`golemd_client.py` (only, at most, historical references in docs/ADRs, which are fine).

- [ ] **Step 6: Commit**

```bash
git add apps/fleet/deploy.py apps/fleet/cli.py apps/fleet/golemd_client.py apps/fleet/tests/test_apply_render.py
git commit -m "feat(fleet): exec golemctl per host on apply; remove sync HTTP apply + ffa1414 stopgap

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** the golemctl resolution order and why no HTTP fallback (stopgap removed, lockstep); that fleet keeps compile + host-selection + manifest-shipping and delegates rendering; the `--json` pass-through; that `_render_report` survives as a printer but not on the apply path.

---

### Task 11: workspace gate + live smoke (operator-verified)

**Files:** none (verification only).

**Interfaces:** none.

- [ ] **Step 1: Full workspace test gate**

Run: `cargo test --workspace`
Expected: PASS — golemd (lib + all integration incl. `async_apply`, migrated `report_api`), golemctl (poll + model + view + apply), and the rest of the workspace. No warnings-as-errors required, but the suite must be green.

- [ ] **Step 2: Fleet test gate**

Run: `python -m pytest apps/fleet/tests/test_apply_render.py apps/fleet/tests/test_ports.py apps/fleet/tests/test_resume.py apps/fleet/tests/test_cloud_init.py -q`
Expected: PASS.

- [ ] **Step 3: Clippy (no new lints)**

Run: `cargo clippy -p golemd -p golemctl`
Expected: clean (the new modules carry `#[allow(clippy::too_many_arguments)]` only where the enact spine already does). Fix any lint the new code introduces.

- [ ] **Step 4: Build golemctl for the smoke run**

Run: `cargo build --workspace`
Expected: `target/debug/golemctl` and `target/debug/golemd` exist (fleet's `resolve_golemctl` finds the former).

- [ ] **Step 5: Live smoke — apply the fishnet-farm to a running scaly via fleet → golemctl (OPERATOR-VERIFIED)**

> **This step is run by the controller (a human operator), not the implementing agent.** It requires a booted `scaly` VM. Mark the checkbox only after the operator confirms the eyeball observations below.

Operator commands (from the repo root, in the devenv shell):

```bash
fleet up scaly
fleet deploy examples/fishnet-farm/farm.emet
fleet apply scaly
```

Eyeball checklist (what "operator-verified" means):
- `fleet apply scaly` returns to a prompt **quickly after the POST** (the 202), then a **live tree** appears — one row per unit under `scaly`, each with a spinner while its glyphs are in progress.
- Log lines stream **under the active unit** as golemd emits them (install lines, and for the canary a warn/enact-failed line).
- On settle: sibling leaves show `✓`, the **canary shows `✗`/partial** (its unpullable image fails to start; `policy = keep`), and the **final report prints** exactly as before (same `outcome`/`units`/`failures`, ADR 0029 shape).
- Re-running `fleet apply scaly` while one is mid-flight prints the **409 "reconcile-in-progress"** message (typed), not a hang or a 500.
- `golemctl apply examples/fishnet-farm/farm.emet http://127.0.0.1:<scaly golemd_port> --json` (run directly) emits plain lines + JSON report, no spinner.

- [ ] **Step 6: Commit (docs/verification only, if anything was tweaked)**

Only if Steps 1–4 required a fix, commit it with the appropriate scope. The smoke step (5) produces no commit — it is a manual gate.

```bash
git add <any-file-fixed-in-steps-1-4>
git commit -m "test: workspace + fleet gates green for live-progress apply

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** none (verification task). If the animated iocraft loop (Task 9's optional upgrade) was adopted, the documenter notes the operator-observed FPS/stderr behaviour.

---

## Self-Review

**1. Spec coverage** (ADR 0033 sections → tasks):
- §1 `POST /manifest` → `202 { reconcile_id }`, cheap ingest gate, spawn: **Task 1** (ingest/run split, ReconcileInProgress), **Task 5** (202 handler + spawn).
- §1 decode/gate failures stay typed non-2xx; unsettled-attempt → 409: **Task 1** (variant), **Task 5** (409 mapping + body carries `reconcile_id`).
- §2 `GET /reconciles/<id>?after=<seq>` projection (phase, per-glyph state, `next_retry_in_ms`, report-on-settle): **Task 2** (projection), **Task 3** (event ring + retry state), **Task 5** (endpoint). `/reconciles/latest`: **Task 5**.
- §2 event log (WAL-derived + in-memory ring, `?after`/`cursor` resumable): **Task 3** (ring), **Task 4** (emission), **Task 5** (cursor semantics).
- §3 golemctl live devenv-style TUI (model/events/view, spinner-per-unit, log-lines-under-active, per-glyph vocabulary, print report on settle): **Task 6** (poll client), **Task 7** (model+fold), **Task 8** (view+components), **Task 9** (live loop). `--reattach`/non-TTY/`--json` fallback: **Task 9** (json + tty branch; reattach noted as a `get_latest` addition — **flagged: `--reattach` flag itself is described in Task 9's control-flow note but the CLI flag wiring is light; if strict reattach is required, add `--reattach` to the `Apply` clap variant and a `get_latest` in poll.rs**).
- §3 iocraft dep at devenv's 0.8 line: **Task 6** (Cargo + git-patch contingency).
- §4 granularity honesty (per-glyph elapsed, no byte bar): honored — the view shows state + optional retry countdown, no progress bar; recorded in Task 8 doc backlog.
- §5 fleet execs golemctl, keeps compile/select/ship, retires `_render_report` from apply path: **Task 10**.
- §6 prerequisite for parallel apply: the 202 makes cross-host fan-out a fleet loop; **not implemented here** (the ADR scopes parallel apply as follow-on) — the sequential per-host loop remains in Task 10, now over cheap execs. Correct per ADR (§6 says cross-host parallelism is a later fleet change; this ADR only decouples ingest).
- §7 lockstep, stopgap removed on landing: **Task 10** (delete `apply_manifest`/`_APPLY_TIMEOUT`/`_render_apply_transport_error`).

**2. Placeholder scan:** No "TBD"/"handle errors"/"similar to Task N". Every code step shows complete code. The two honest hedges (iocraft timer/render API on 0.8.2; the string-redraw vs animated iocraft loop) are explicitly framed as build-out contingencies with the tested surface held stable — not placeholders, but recorded seams the ADR itself leaves to implementation.

**3. Type consistency:** `reconcile_id: u64` throughout. `ProgressEvent`/`EventLevel` defined in Task 3, consumed by Task 2's `project` and Task 4's emission (dependency noted: land Task 3 before Task 2's Step 5). `ReconcileProgress`/`GlyphState`/`PhaseView` (golemd, `projection.rs`) mirror golemctl's `Progress`/`GlyphState`/`Phase` (`poll.rs`) — same snake_case serde tags, verified against the JSON contract in Task 5. `resolve_golemctl` (Task 10 deploy.py) matches the `cli.deploy_ops.resolve_golemctl` mock in the test. `apply_progress`/`is_settled`/`should_stop`/`plain_line`/`render_to_string`/`view`/`glyph_mark`/`unit_mark` names consistent across Tasks 7–9.

**Narrowed / infeasible in the ADR (recorded):**
- **`phase: "settling"`** — the ADR lists a `settling` phase, but the current `AttemptPhase` enum has no such value (`Planning/Enacting/RollingBack/Committed/RolledBack`). The projection defines `PhaseView::Settling` for forward-compat but **never emits it today** (Committed→Settled). Narrowed honestly in Task 2.
- **`ManifestUndecodable` status** — the ADR calls decode failures "400-class"; the existing code returns **500** for `ManifestUndecodable`. This plan does **not** re-taxonomize it (only adds the 409); Task 5 keeps the 500 assertion. Recorded as an intentional non-change to avoid touching the error taxonomy the ADR did not require changing.
- **Report durability across restart** — the settled `ReconcileReport` is cached in-memory (Task 5), lost on restart like the event ring. The ADR accepts this asymmetry (states durable via WAL/recovery; transient lines best-effort); the report cache is the same trade. If a restart-durable report is later required, it can be re-projected from the settled revision + WAL — flagged, not built.
- **`--reattach` flag** — the ADR mentions `golemctl apply --reattach`; this plan implements the reattach *capability* (`/reconciles/latest` on golemd, and the poll client can hit it) but leaves the explicit CLI flag wiring light (Task 9 note). If required as a hard deliverable, add the flag + `get_latest` — a small addition, called out above.
