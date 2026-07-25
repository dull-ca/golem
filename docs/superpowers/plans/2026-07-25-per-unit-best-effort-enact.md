# Per-Unit Best-Effort Enact Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace golemd's fail-fast, per-op retry with a **per-leaf-unit best-effort** enact: each leaf unit of the recursive host scroll is enacted best-effort, its failing-retryable ops retried on a unit-level round loop with backoff+jitter under dual limits, and — on exhaustion — settled by its own resolved `on_exhaust` (`rollback` scoped to the unit, or `keep`), with siblings untouched. golemd loads a `golemd.toml [retry]` default (overridable per scroll by the ADR 0031 policy cascade), returns a tree-shaped `ReconcileReport` at HTTP 200 in all cases, wraps WAL/manifest read failures in typed `{ kind, message }` errors, and `fleet apply` renders per-unit blocks and typed errors. Removes emit in reverse source order.

**Architecture:** This plan sits **on top of Plan 1** (`docs/superpowers/plans/2026-07-25-recursive-scroll-model.md`), which already gave `Scroll` a recursive tree, a per-scroll `Policy` on the wire, and a `unit_path` on every WAL step. Plan 2 changes only `apps/golemd/` and `apps/fleet/`: a new `config.rs` (`RetryConfig` + TOML loader + `--config`), a rewritten `foreman::enact` that walks `scroll.leaf_units()` and runs a WAL-fold-driven round loop per unit, a policy-cascade resolver folding `golemd.toml` → ancestor policies → leaf policy into an effective `RetryConfig`, a per-unit `on_exhaust` branch reusing `rollback_attempt` scoped by `unit_path`, a tree-shaped `ReconcileReport`, typed `ForemanError`, structured `ApiError` bodies, and a `fleet apply` renderer. The WAL bracketing, recovery fold, and `Reconciler` port are reused unchanged.

**Tech Stack:** Rust (`serde` + `toml` for config, `rusqlite` WAL, `fastrand` for jitter — already a workspace dependency, `tracing` for immediate logging, `axum` HTTP), Python `typer`/`rich` fleet CLI.

## Global Constraints

- **Zero comments in implementation code.** Every code snippet here has no comments; implementers add none. A documenter agent owns all comments/prose afterward. Each task ends with a **Doc backlog** line.
- **TDD, red-green, every behavior.** Failing test first, run it to see the stated failure, minimal implementation, run green. Use each crate's existing test locations (golemd in-crate `#[cfg(test)] mod tests` and `apps/golemd/tests/*.rs`; fleet under `apps/fleet/tests/`).
- **The wire field/variant order in ADR 0031 §5 is normative** (postcard is non-self-describing). This plan does **not** change the wire types (Plan 1 fixed them); `RetryConfig`, `ReconcileReport`, `UnitReport`, `GlyphFailure`, and `ForemanError` are golemd's private operational/API surfaces (serde_json / TOML), **not** part of the manifest wire contract. `golemd.toml` is never hashed and never crosses the manifest wire. The per-scroll `Policy` that overrides it *is* on the wire (delivered by Plan 1).
- **`main : List Scroll` and existing flat `scroll { name, glyphs }` programs keep working.** A flat scroll is a single leaf unit; its enact and rollback outcome under the default `on_exhaust = rollback` are identical to today's whole-scroll behavior (reached after best-effort tries everything).
- **`on_exhaust` defaults to `rollback`** (ADR 0029 §4 / ADR 0031 §3). A field unset at every scope falls to the `golemd.toml` value, which falls to the built-in default (`rollback`).
- **Build/test commands (Cargo workspace under a nix `devenv`; `cargo` is on PATH in the dev shell):**
  - `cargo test -p golemd` / `cargo test --workspace`
  - `cargo test -p golemd <test_name>` / `cargo test -p golemd --lib` / `cargo test -p golemd --test <file>`
  - `cargo build --release -p golemd -p golemctl -p emet`
  - Fleet tests: from repo root, `PYTHONPATH=apps python -m unittest discover apps/fleet/tests` (the suite is `unittest`-style).
- **Git discipline.** Never touch the `result` symlink; never `git push`. Commit with `git add <specific paths>` (never `-A`). Every commit message ends with:
  `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`

## Interfaces consumed from Plan 1 (exact signatures)

These land in Plan 1 and Plan 2 depends on them verbatim:

- `scroll_format::Scroll { pub name: String, pub policy: Option<Policy>, pub contents: Contents }`
- `scroll_format::Contents { Glyphs(Vec<Glyph>), Groups(Vec<Scroll>) }`
- `scroll_format::Policy { pub base_delay_ms: Option<u64>, pub backoff_multiplier: Option<f64>, pub max_delay_ms: Option<u64>, pub jitter_fraction: Option<f64>, pub max_attempts: Option<u32>, pub max_elapsed_ms: Option<u64>, pub on_exhaust: Option<OnExhaust> }` (derives `Default`)
- `scroll_format::OnExhaust { Rollback, Keep }` (derives `Clone, Copy, PartialEq, Eq`)
- `impl Scroll { pub fn leaf_units(&self) -> Vec<LeafUnit<'_>> }`
- `scroll_format::LeafUnit<'a> { pub path: Vec<String>, pub glyphs: &'a [Glyph], pub policy_chain: Vec<&'a Policy> }` — `path` is root→leaf names; `policy_chain` is root-most first, the leaf's own policy last.
- `impl Scroll { pub fn glyphs(&self) -> &[Glyph]; pub fn all_glyphs(&self) -> Vec<&Glyph>; pub fn is_leaf(&self) -> bool }`
- `WalStep { …, pub unit_path: Vec<String> }` (Plan 1 Task 11)
- `PlanRoom::append_wal_step(&self, reconcile_id: u64, step_ord: u64, glyph_key: &str, action: WalAction, state: WalStepState, op: &GlyphOp, inverse: Option<&Inverse>, changed: Option<bool>, unit_path: &[String]) -> Result<WalStep>` (Plan 1 Task 11; `unit_path` is the new final parameter)

Signatures this plan reuses **unchanged** from the current code:
- `reconcile::plan(prior: &[Outcome], desired: &Scroll) -> Vec<GlyphOp>`
- `Foreman::rollback_attempt(&self, reconcile_id: u64) -> Result<()>` (this plan adds a `unit_path`-scoped variant)
- `Foreman::apply_manifest(&self, bytes: &[u8]) -> Result<…>` (return type changes to `ReconcileReport` in Task 8)
- `wal::applied_outcomes(steps: &[WalStep]) -> Vec<Outcome>`
- `reconciler::EnactError { Retryable(String), Fatal(String) }`, `EnactResult<T>`
- `Reconciler::apply(&self, glyph: &Glyph, cid: ContentId) -> EnactResult<Outcome>`, `Reconciler::reverse(&self, outcome: &Outcome) -> EnactResult<()>`

---

## File Structure

- **`apps/golemd/src/config.rs`** — NEW. `RetryConfig` (serde + toml, `Default`), `load(path: Option<&Path>) -> Result<RetryConfig, ConfigError>` file loader.
- **`apps/golemd/src/main.rs`** — `--config` flag; load `RetryConfig`; `Foreman::with_retry_config`.
- **`apps/golemd/src/foreman.rs`** — delete `attempt`/`attempt_reverse`; rewrite `enact` into a per-leaf-unit walk + round loop; policy-cascade resolver; per-unit `on_exhaust` (rollback scoped by `unit_path`); immediate `warn!`/`error!` at Failed arms; `apply_manifest` returns `ReconcileReport`; typed `ForemanError`.
- **`apps/golemd/src/report.rs`** — NEW. `ReconcileReport`, `UnitReport`, `GlyphFailure`, outcome enums (serde `Serialize`).
- **`apps/golemd/src/reconcile.rs`** — removes emitted in reverse source order.
- **`apps/golemd/src/http.rs`** — 200-with-report for reconcile; structured `{ kind, message }` `ApiError` body; map `ForemanError` to HTTP 500 with a stable `kind`.
- **`apps/golemd/src/lib.rs`** — declare `config` and `report` modules.
- **`apps/fleet/cli.py`** — `apply` renders per-unit blocks + failures + typed errors.
- **`apps/fleet/tests/test_apply_render.py`** — NEW. Mock the report shapes, assert the render.
- **Docs** — the ordering-contract note in `apps/emet/CLAUDE.md` (documenter-owned; named in Task 10).

---

## Task 1: golemd — `config.rs` with `RetryConfig`, TOML loader, and defaults

**Files:**
- Create: `apps/golemd/src/config.rs`
- Modify: `apps/golemd/src/lib.rs`
- Test: `apps/golemd/src/config.rs` in-crate `#[cfg(test)] mod tests`

**Interfaces:**
- Produces:
  - `pub struct RetryConfig { pub base_delay_ms: u64, pub backoff_multiplier: f64, pub max_delay_ms: u64, pub jitter_fraction: f64, pub max_attempts: u32, pub max_elapsed_ms: u64, pub on_exhaust: OnExhaustConfig }`
  - `pub enum OnExhaustConfig { Rollback, Keep }` (serde `rename_all = "lowercase"`, mapping `"rollback"`/`"keep"`)
  - `impl Default for RetryConfig` — the built-in defaults (`base_delay_ms: 200`, `backoff_multiplier: 2.0`, `max_delay_ms: 30_000`, `jitter_fraction: 0.2`, `max_attempts: 5`, `max_elapsed_ms: 120_000`, `on_exhaust: Rollback`).
  - `pub fn load(path: Option<&std::path::Path>) -> Result<RetryConfig, ConfigError>` — `None` → `RetryConfig::default()`; `Some(p)` → parse the `[retry]` table, each present field overriding its default.
  - `pub enum ConfigError { Read(String), Parse(String) }` (impls `Display`, `std::error::Error`).

- [ ] **Step 1: Declare the module**

In `apps/golemd/src/lib.rs`, add to the module list:

```rust
pub mod config;
```

- [ ] **Step 2: Write the failing tests**

Create `apps/golemd/src/config.rs`:

```rust
use std::path::Path;

use scroll_format::OnExhaust;
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OnExhaustConfig {
    Rollback,
    Keep,
}

impl OnExhaustConfig {
    pub fn to_on_exhaust(self) -> OnExhaust {
        match self {
            OnExhaustConfig::Rollback => OnExhaust::Rollback,
            OnExhaustConfig::Keep => OnExhaust::Keep,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RetryConfig {
    pub base_delay_ms: u64,
    pub backoff_multiplier: f64,
    pub max_delay_ms: u64,
    pub jitter_fraction: f64,
    pub max_attempts: u32,
    pub max_elapsed_ms: u64,
    pub on_exhaust: OnExhaustConfig,
}

impl Default for RetryConfig {
    fn default() -> Self {
        RetryConfig {
            base_delay_ms: 200,
            backoff_multiplier: 2.0,
            max_delay_ms: 30_000,
            jitter_fraction: 0.2,
            max_attempts: 5,
            max_elapsed_ms: 120_000,
            on_exhaust: OnExhaustConfig::Rollback,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct FileShape {
    retry: Option<RetryTable>,
}

#[derive(Debug, Default, Deserialize)]
struct RetryTable {
    base_delay_ms: Option<u64>,
    backoff_multiplier: Option<f64>,
    max_delay_ms: Option<u64>,
    jitter_fraction: Option<f64>,
    max_attempts: Option<u32>,
    max_elapsed_ms: Option<u64>,
    on_exhaust: Option<OnExhaustConfig>,
}

#[derive(Debug)]
pub enum ConfigError {
    Read(String),
    Parse(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Read(m) => write!(f, "could not read golemd config: {m}"),
            ConfigError::Parse(m) => write!(f, "could not parse golemd config: {m}"),
        }
    }
}

impl std::error::Error for ConfigError {}

pub fn load(path: Option<&Path>) -> Result<RetryConfig, ConfigError> {
    let Some(path) = path else {
        return Ok(RetryConfig::default());
    };
    let text = std::fs::read_to_string(path).map_err(|e| ConfigError::Read(e.to_string()))?;
    let shape: FileShape = toml::from_str(&text).map_err(|e| ConfigError::Parse(e.to_string()))?;
    let mut cfg = RetryConfig::default();
    if let Some(t) = shape.retry {
        if let Some(v) = t.base_delay_ms { cfg.base_delay_ms = v; }
        if let Some(v) = t.backoff_multiplier { cfg.backoff_multiplier = v; }
        if let Some(v) = t.max_delay_ms { cfg.max_delay_ms = v; }
        if let Some(v) = t.jitter_fraction { cfg.jitter_fraction = v; }
        if let Some(v) = t.max_attempts { cfg.max_attempts = v; }
        if let Some(v) = t.max_elapsed_ms { cfg.max_elapsed_ms = v; }
        if let Some(v) = t.on_exhaust { cfg.on_exhaust = v; }
    }
    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_path_gives_builtin_defaults() {
        let cfg = load(None).unwrap();
        assert_eq!(cfg, RetryConfig::default());
        assert_eq!(cfg.max_attempts, 5);
        assert_eq!(cfg.on_exhaust, OnExhaustConfig::Rollback);
    }

    #[test]
    fn present_fields_override_defaults_absent_fields_do_not() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("golemd.toml");
        std::fs::write(
            &path,
            "[retry]\nmax_attempts = 9\non_exhaust = \"keep\"\n",
        )
        .unwrap();
        let cfg = load(Some(&path)).unwrap();
        assert_eq!(cfg.max_attempts, 9);
        assert_eq!(cfg.on_exhaust, OnExhaustConfig::Keep);
        assert_eq!(cfg.base_delay_ms, 200);
        assert_eq!(cfg.backoff_multiplier, 2.0);
    }

    #[test]
    fn a_malformed_config_is_a_typed_parse_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("golemd.toml");
        std::fs::write(&path, "[retry]\nmax_attempts = \"lots\"\n").unwrap();
        match load(Some(&path)) {
            Err(ConfigError::Parse(_)) => {}
            other => panic!("expected Parse, got {other:?}"),
        }
    }
}
```

- [ ] **Step 3: Add the `toml` dependency**

`toml` is not yet a workspace dependency. In the root `Cargo.toml` `[workspace.dependencies]`, add:

```toml
toml = "0.8"
```

In `apps/golemd/Cargo.toml` `[dependencies]`, add `toml = { workspace = true }`. Confirm `tempfile = { workspace = true }` is in golemd's `[dev-dependencies]` (it is a workspace dep; add it under `[dev-dependencies]` if absent, since the config test uses it).

- [ ] **Step 4: Run the tests**

Run: `cargo test -p golemd --lib config::tests`
Expected: the three tests pass.

- [ ] **Step 5: Commit**

```bash
git add apps/golemd/src/config.rs apps/golemd/src/lib.rs Cargo.toml apps/golemd/Cargo.toml
git commit -m "feat(golemd): golemd.toml retry config with defaults and typed load errors

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** document the `golemd.toml [retry]` table (all fields optional; shown defaults are built-in), that the file is golemd's private operational surface (never on the manifest wire), that absent → defaults so CLI-only invocation keeps working, and the field semantics (delay/backoff/jitter/dual-limits/on_exhaust). Add the `[retry]` example block from ADR 0029 §3.

---

## Task 2: golemd — wire `--config` and `with_retry_config` into `main.rs` and `Foreman`

**Files:**
- Modify: `apps/golemd/src/main.rs`
- Modify: `apps/golemd/src/foreman.rs`
- Test: `apps/golemd/src/foreman.rs` in-crate test (a `Foreman` built with a custom `RetryConfig` exposes it).

**Interfaces:**
- Consumes: Task 1 `RetryConfig`, `config::load`.
- Produces: `impl Foreman { pub fn with_retry_config(self, cfg: RetryConfig) -> Self }`; the `Foreman` struct's `max_attempts: u32` + `retry_delay: Duration` fields (lines 49–50) are replaced by `retry: RetryConfig`.

- [ ] **Step 1: Write the failing test**

In `apps/golemd/src/foreman.rs`'s test module, add:

```rust
    #[test]
    fn with_retry_config_is_stored() {
        let planroom = Box::new(crate::planroom::SqlitePlanRoom::open(std::path::Path::new(":memory:")).unwrap());
        let foreman = Foreman::new("h".into(), planroom, Box::new(Recorder::default()))
            .with_retry_config(crate::config::RetryConfig { max_attempts: 9, ..Default::default() });
        assert_eq!(foreman.retry.max_attempts, 9);
    }
```

Match the existing test-module helpers (`Recorder` exists per the studied foreman tests; the in-memory planroom is `SqlitePlanRoom::open(Path::new(":memory:"))` — reuse whatever constructor the current foreman tests use).

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p golemd --lib with_retry_config_is_stored`
Expected: compile error — `retry` field and `with_retry_config` do not exist.

- [ ] **Step 3: Replace the retry fields on `Foreman`**

In `apps/golemd/src/foreman.rs`, replace the two fields `max_attempts: u32,` and `retry_delay: Duration,` (lines 49–50) with:

```rust
    retry: RetryConfig,
```

Add `use crate::config::RetryConfig;` to the imports. In `Foreman::new` (lines 62–75), replace the `max_attempts: 5, retry_delay: Duration::from_millis(200),` initializers with `retry: RetryConfig::default(),`. Replace the `with_retry` builder (lines 77–81) with:

```rust
    pub fn with_retry_config(mut self, cfg: RetryConfig) -> Self {
        self.retry = cfg;
        self
    }
```

(Any test still calling `.with_retry(max, delay)` is updated in this task's Step 5 or Task 5; grep for callers.)

- [ ] **Step 4: Add the `--config` flag and load it in `main.rs`**

In `apps/golemd/src/main.rs`, add a field to `Cli` (after `reconciler`, line 39):

```rust
    #[arg(long)]
    config: Option<PathBuf>,
```

After constructing the reconciler and before `Foreman::new`, load the config and pass it in (lines 52–57 region):

```rust
    let retry = golemd::config::load(cli.config.as_deref())
        .with_context(|| "load golemd config")?;
    let foreman = Arc::new(
        Foreman::new(cli.host.clone(), Box::new(planroom), reconciler).with_retry_config(retry),
    );
```

- [ ] **Step 5: Update any remaining `with_retry` callers**

Run: `rg -n 'with_retry\b|max_attempts|retry_delay' apps/golemd/src apps/golemd/tests`

Replace each `.with_retry(m, d)` call with `.with_retry_config(RetryConfig { max_attempts: m, base_delay_ms: <d as ms>, ..Default::default() })`, and any `self.max_attempts`/`self.retry_delay` reads inside `foreman.rs` (in `attempt`/`attempt_reverse`, which are deleted in Task 4) — those are removed with the functions. Fix compile errors so `cargo build -p golemd` is clean.

- [ ] **Step 6: Run**

Run: `cargo test -p golemd --lib with_retry_config_is_stored`
Expected: PASS. And `cargo build -p golemd` clean.

- [ ] **Step 7: Commit**

```bash
git add apps/golemd/src/main.rs apps/golemd/src/foreman.rs
git commit -m "feat(golemd): --config flag and Foreman::with_retry_config

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** document the `--config` flag (names a non-default `golemd.toml`), and that `Foreman` now holds a `RetryConfig` fleet default that the per-scroll policy cascade overrides.

---

## Task 3: golemd — the policy-cascade resolver (config → ancestors → leaf → effective `RetryConfig`)

**Files:**
- Modify: `apps/golemd/src/foreman.rs` (add a free function `resolve_retry`)
- Test: `apps/golemd/src/foreman.rs` in-crate tests.

**Interfaces:**
- Consumes: Task 1 `RetryConfig`/`OnExhaustConfig`, Plan 1 `Policy`/`OnExhaust`/`LeafUnit`.
- Produces: `pub(crate) fn resolve_retry(base: &RetryConfig, policy_chain: &[&Policy]) -> RetryConfig` — folds the fleet default with the ancestor-to-leaf policy chain, each set field overriding the wider scope (nearest wins; the chain is root-most first so folding left-to-right lets the leaf win).

- [ ] **Step 1: Write the failing tests**

In `apps/golemd/src/foreman.rs` test module:

```rust
    #[test]
    fn resolve_retry_uses_config_when_no_policy() {
        let base = crate::config::RetryConfig { max_attempts: 5, ..Default::default() };
        let eff = super::resolve_retry(&base, &[]);
        assert_eq!(eff.max_attempts, 5);
        assert_eq!(eff.on_exhaust, crate::config::OnExhaustConfig::Rollback);
    }

    #[test]
    fn resolve_retry_leaf_overrides_ancestor_overrides_config() {
        use scroll_format::{OnExhaust, Policy};
        let base = crate::config::RetryConfig { max_attempts: 5, ..Default::default() };
        let ancestor = Policy { max_attempts: Some(8), on_exhaust: Some(OnExhaust::Rollback), ..Policy::default() };
        let leaf = Policy { on_exhaust: Some(OnExhaust::Keep), ..Policy::default() };
        let eff = super::resolve_retry(&base, &[&ancestor, &leaf]);
        assert_eq!(eff.max_attempts, 8);
        assert_eq!(eff.on_exhaust, crate::config::OnExhaustConfig::Keep);
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p golemd --lib resolve_retry_uses_config_when_no_policy`
Expected: compile error — `resolve_retry` does not exist.

- [ ] **Step 3: Implement `resolve_retry`**

Add to `apps/golemd/src/foreman.rs` (a free function, not a method):

```rust
pub(crate) fn resolve_retry(base: &RetryConfig, policy_chain: &[&scroll_format::Policy]) -> RetryConfig {
    let mut cfg = *base;
    for policy in policy_chain {
        if let Some(v) = policy.base_delay_ms { cfg.base_delay_ms = v; }
        if let Some(v) = policy.backoff_multiplier { cfg.backoff_multiplier = v; }
        if let Some(v) = policy.max_delay_ms { cfg.max_delay_ms = v; }
        if let Some(v) = policy.jitter_fraction { cfg.jitter_fraction = v; }
        if let Some(v) = policy.max_attempts { cfg.max_attempts = v; }
        if let Some(v) = policy.max_elapsed_ms { cfg.max_elapsed_ms = v; }
        if let Some(v) = policy.on_exhaust {
            cfg.on_exhaust = match v {
                scroll_format::OnExhaust::Rollback => crate::config::OnExhaustConfig::Rollback,
                scroll_format::OnExhaust::Keep => crate::config::OnExhaustConfig::Keep,
            };
        }
    }
    cfg
}
```

- [ ] **Step 4: Run**

Run: `cargo test -p golemd --lib resolve_retry`
Expected: both tests pass.

- [ ] **Step 5: Commit**

```bash
git add apps/golemd/src/foreman.rs
git commit -m "feat(golemd): resolve effective per-unit RetryConfig from the policy cascade

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** document the cascade order (golemd.toml default → ancestor branch policies root-to-leaf → leaf policy, nearest wins), matching ADR 0031 §3 / ADR 0029 §3; that an unset field at every scope falls to the config default which falls to the built-in.

---

## Task 4: golemd — the per-unit best-effort round loop (delete `attempt`/`attempt_reverse`)

**Files:**
- Modify: `apps/golemd/src/foreman.rs`
- Test: `apps/golemd/src/foreman.rs` in-crate tests (best-effort within a unit; sibling isolation).

**Interfaces:**
- Consumes: Task 3 `resolve_retry`, Plan 1 `Scroll::leaf_units()`/`LeafUnit`, the WAL `unit_path` from Plan 1 Task 11, `reconcile::plan`.
- Produces: a rewritten enact spine. `enact_apply`/`enact_reverse` call the reconciler **once** and return the classified `EnactResult` to the round loop; `attempt`/`attempt_reverse` (lines 456–488) are **deleted**. New per-unit driver returns, per unit, the set of `GlyphFailure`s (defined in Task 7's `report.rs`; for Task 4 use a placeholder internal struct `UnitFailure { glyph_key: String, phase: Phase, class: FailClass, attempts: u32, message: String }` that Task 7 maps into `GlyphFailure`).

Because the enact rewrite, `on_exhaust`, and the report are tightly coupled, Tasks 4–8 build one spine incrementally. Task 4 lands the **round loop and best-effort semantics within a single unit and across sibling units**, returning an internal failure list; Task 5 adds the delay/backoff/jitter/dual-limit timing; Task 6 adds per-unit `on_exhaust`; Task 7 defines the report types; Task 8 assembles `apply_manifest` → `ReconcileReport`.

- [ ] **Step 1: Write the failing test — best-effort within a unit**

In the foreman test module, using the existing fake-reconciler test scaffolding (`Recorder`, `FlakyThenOk`, `Failing` — reuse them), add a test that two independent ops in one unit are both attempted even when the first fails fatally, and a sibling unit is untouched:

```rust
    #[test]
    fn a_fatal_glyph_does_not_veto_the_rest_of_its_unit() {
        let reconciler = ScriptedReconciler::new()
            .fatal_on("apt:bad")
            .ok_default();
        let foreman = foreman_with(reconciler);
        let scroll = leaf_scroll("unit", vec![apt("bad"), apt("good")]);
        let report = foreman.apply_scroll(scroll).unwrap();
        assert_eq!(report.units.len(), 1);
        assert_eq!(report.units[0].failures.len(), 1);
        assert_eq!(report.units[0].failures[0].glyph_key, "apt:bad");
        assert!(applied_keys(&foreman).contains(&"apt:good".to_string()));
    }

    #[test]
    fn one_unit_failing_leaves_a_sibling_unit_settled() {
        let reconciler = ScriptedReconciler::new().fatal_on("apt:bad").ok_default();
        let foreman = foreman_with(reconciler);
        let scroll = branch_scroll(
            "host",
            vec![
                leaf_scroll("broken", vec![apt("bad")]),
                leaf_scroll("healthy", vec![apt("good")]),
            ],
        );
        let report = foreman.apply_scroll(scroll).unwrap();
        let healthy = report.units.iter().find(|u| u.unit_path.last().unwrap() == "healthy").unwrap();
        assert!(healthy.failures.is_empty());
        assert!(applied_keys(&foreman).contains(&"apt:good".to_string()));
    }
```

These reference test helpers that do not yet exist (`ScriptedReconciler`, `foreman_with`, `leaf_scroll`, `branch_scroll`, `apply_scroll`, `applied_keys`, `report.units`). Add them to the test module. `apply_scroll` is a thin test helper that builds a one-scroll manifest and calls `apply_manifest`, returning the `ReconcileReport`; since `ReconcileReport` arrives in Task 7, for Task 4 have `apply_scroll` return the internal per-unit failure structure and rename to `report.units` after Task 7. To keep Task 4 self-contained, define the internal return type now (see Step 3) and let Task 8 fold it into the public report.

Model `ScriptedReconciler` on the existing `Recorder`/`FlakyThenOk` fakes (read them first): it records applied keys and can be told a key fails `Fatal`/`Retryable` a given number of times.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p golemd --lib a_fatal_glyph_does_not_veto_the_rest_of_its_unit`
Expected: compile error (missing helpers and the new enact structure).

- [ ] **Step 3: Rewrite the enact spine**

Replace `Foreman::enact` (lines 158–191) and its callees. The new structure:

Add internal types near the top of `foreman.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum FailClass {
    Fatal,
    RetriesExhausted,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Phase {
    Enact,
    Reverse,
}

#[derive(Debug, Clone)]
pub(crate) struct UnitFailure {
    pub glyph_key: String,
    pub unit_path: Vec<String>,
    pub phase: Phase,
    pub class: FailClass,
    pub attempts: u32,
    pub message: String,
}

pub(crate) struct UnitResult {
    pub unit_path: Vec<String>,
    pub failures: Vec<UnitFailure>,
    pub rolled_back: bool,
}
```

Rewrite `enact` to walk leaf units and enact each best-effort. Its signature becomes a per-unit driver; `reconcile` (Task 6/8) calls it once per unit. For Task 4, land the within-unit round loop with a **single round** (no delay yet — timing is Task 5) so best-effort and continue-on-failure are proven first:

```rust
    fn enact_unit(
        &self,
        reconcile_id: u64,
        ops: &[GlyphOp],
        prior: &[Outcome],
        unit_path: &[String],
        retry: &RetryConfig,
    ) -> Result<Vec<UnitFailure>> {
        for (ord, op) in ops.iter().enumerate() {
            self.enact_one(reconcile_id, ord as u64, op, prior, unit_path)?;
        }
        let mut round = 1u32;
        loop {
            let remaining = self.remaining_ops(reconcile_id, unit_path, ops);
            if remaining.is_empty() {
                break;
            }
            if round + 1 > retry.max_attempts {
                break;
            }
            round += 1;
            for (ord, op) in remaining {
                self.enact_one(reconcile_id, ord, &op, prior, unit_path)?;
            }
        }
        Ok(self.unit_failures(reconcile_id, unit_path, ops, retry))
    }
```

Add `enact_one` — one op, one WAL bracket, one reconciler call, classify but **do not propagate**:

```rust
    fn enact_one(
        &self,
        reconcile_id: u64,
        ord: u64,
        op: &GlyphOp,
        prior: &[Outcome],
        unit_path: &[String],
    ) -> Result<()> {
        match op {
            GlyphOp::Noop { .. } => Ok(()),
            GlyphOp::Install { cid, glyph } => {
                self.enact_apply(reconcile_id, ord, op, glyph, *cid, None, unit_path)
            }
            GlyphOp::Replace { new_cid, glyph, .. } => {
                if replaces_in_place(glyph) {
                    self.enact_apply(reconcile_id, ord, op, glyph, *new_cid, None, unit_path)
                } else {
                    let prior_outcome = self.prior_outcome(op, prior);
                    let _ = self.enact_reverse(reconcile_id, ord, op, &prior_outcome, unit_path);
                    self.enact_apply(reconcile_id, ord, op, glyph, *new_cid, None, unit_path)
                }
            }
            GlyphOp::Remove { glyph, .. } => {
                let prior_outcome = self.prior_outcome(op, prior);
                self.enact_reverse(reconcile_id, ord, op, &prior_outcome, unit_path)
            }
        }
    }
```

Change `enact_apply`/`enact_reverse` (Plan 1 already added their `unit_path` param) so that on a `Failed` bracket they **return `Ok(())`** (the failure lives in the WAL, read by `remaining_ops`/`unit_failures`), except propagate a genuine planroom I/O error with `?`. Concretely, replace the `Err(e) => { …append Failed…; Err(e) }` arms with an arm that appends the `Failed` row, logs immediately (Task 6 adds the log lines), and returns `Ok(())` while stashing the classified reason so `unit_failures` can read it from the WAL. Because the WAL `Failed` row does not itself carry the `EnactError` class, record the class by writing it into the `message`/inspecting `EnactError` at the call site — simplest: have `enact_apply` return `Result<Option<AttemptOutcome>>` where `AttemptOutcome` captures `(class, message)` on failure; but to keep the WAL-fold-as-truth invariant, prefer reading failure from the WAL. Implementation choice, **recommended**: extend the classification into the WAL by storing the failure class in a new nullable `fail_class` reasoning column — but that widens the schema. **Chosen approach (no schema change):** `enact_apply`/`enact_reverse` return `Result<StepClass>` where `enum StepClass { Ok, Failed(FailClass, String) }`, and the round loop collects `(ord, StepClass)` in-memory for the current attempt while the WAL remains the crash-recovery truth (a crash mid-attempt is recovered whole by the existing recovery path, which does not need the class). Update the two functions to return `StepClass` and never `Err` on a reconciler failure (only on planroom I/O).

Add `remaining_ops` — read this attempt's WAL filtered to `unit_path`, return the ops whose latest terminal row is `Failed` **and** whose failure class was `Retryable`. Since the WAL does not store the class, track the retryable set in-memory across rounds within the attempt (the in-memory `(ord, StepClass)` map from the round). `remaining_ops` becomes: from the current attempt's in-memory classification, the ops still `Failed(FailClass::…)` that were `Retryable`. Represent this as a method over the round loop's accumulator rather than a WAL re-read for the *class*, while still bracketing every attempt in the WAL. (This honors ADR 0029 §1's WAL-fold-for-recovery while keeping the retryable/fatal *class* — which the WAL never stored even before this change — in memory for the live loop.)

Add `unit_failures` — after the round loop, produce a `Vec<UnitFailure>` from the terminal in-memory classification: each op still failing is `Fatal` (class `Fatal`) or, if it was retryable but the limit tripped, `RetriesExhausted`, with `attempts = round count`.

Delete `attempt` and `attempt_reverse` (lines 456–488).

- [ ] **Step 4: Update `enact_apply`/`enact_reverse` return type and Failed arms**

In `enact_apply` (Plan 1 shape, now with `unit_path`), replace `match self.attempt(op, || self.reconciler.apply(glyph, cid))` with a single call and classification:

```rust
        match self.reconciler.apply(glyph, cid) {
            Ok(outcome) => {
                self.planroom.append_wal_step(
                    reconcile_id, ord, &op.key(), WalAction::Apply, WalStepState::Done,
                    op, Some(&outcome.inverse), Some(outcome.changed), unit_path,
                )?;
                Ok(StepClass::Ok)
            }
            Err(e) => {
                self.planroom.append_wal_step(
                    reconcile_id, ord, &op.key(), WalAction::Apply, WalStepState::Failed,
                    op, None, None, unit_path,
                )?;
                Ok(classify(e))
            }
        }
```

with `fn classify(e: EnactError) -> StepClass { match e { EnactError::Fatal(m) => StepClass::Failed(FailClass::Fatal, m), EnactError::Retryable(m) => StepClass::Failed(FailClass::Retryable, m) } }` and `enum StepClass { Ok, Failed(FailClass, String) }`, `enum FailClass { Fatal, Retryable }`. Return type of `enact_apply`/`enact_reverse` becomes `Result<StepClass>`. Same rewrite in `enact_reverse` with `WalAction::Reverse`. `enact_one` collects the `StepClass` per ord.

(Reconcile the `FailClass` above with Task 4 Step 3's terminal `FailClass { Fatal, RetriesExhausted }`: use two enums — `StepClass::Failed` carries an in-flight `RetryClass { Fatal, Retryable }`; `UnitFailure.class` is the terminal `FailClass { Fatal, RetriesExhausted }`. `unit_failures` maps: a still-failing `Fatal` → `FailClass::Fatal`; a still-failing `Retryable` that hit the limit → `FailClass::RetriesExhausted`.)

- [ ] **Step 5: Run the best-effort tests**

Run: `cargo test -p golemd --lib a_fatal_glyph_does_not_veto_the_rest_of_its_unit one_unit_failing_leaves_a_sibling_unit_settled`
Expected: PASS (once `reconcile`/`apply_scroll` in Task 6/8 wire `enact_unit` per leaf; for Task 4 have the test helper `apply_scroll` call a temporary `reconcile_units` that walks `leaf_units()` and calls `enact_unit` per unit, collecting `UnitResult`s — this temporary is finalized in Task 8).

- [ ] **Step 6: Commit**

```bash
git add apps/golemd/src/foreman.rs
git commit -m "feat(golemd): per-unit best-effort round loop; delete per-op attempt spines

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** document the one spine at the unit level (ADR 0029 §1); `Fatal` is terminal, `Retryable` re-driven; a `Failed` row no longer aborts the loop; every op still `Intended`→`Done`/`Failed` bracketed; sibling units are isolated; why the retryable/fatal *class* is tracked in-memory for the live loop while the WAL remains recovery truth.

---

## Task 5: golemd — backoff + jitter + dual limits between rounds

**Files:**
- Modify: `apps/golemd/src/foreman.rs`
- Test: `apps/golemd/src/foreman.rs` in-crate tests (a retryable-then-ok glyph succeeds within the limit; the delay is bounded; `max_elapsed_ms` trips).

**Interfaces:**
- Consumes: Task 4 round loop, Task 3 `RetryConfig`.
- Produces: `fn round_delay(retry: &RetryConfig, round: u32) -> Duration` — `min(max_delay_ms, base_delay_ms × backoff_multiplier^(round-1))` then perturbed by ± `jitter_fraction` (uniform, via `fastrand`); the round loop sleeps this between rounds and stops when the remaining set is empty **or** `max_attempts` rounds reached **or** `max_elapsed_ms` wall-time from attempt open exceeded.

- [ ] **Step 1: Write the failing tests**

```rust
    #[test]
    fn a_retryable_glyph_succeeds_within_the_attempt_limit() {
        let reconciler = ScriptedReconciler::new().retryable_times("apt:flaky", 2).ok_default();
        let foreman = foreman_with(reconciler)
            .with_retry_config(crate::config::RetryConfig { max_attempts: 5, base_delay_ms: 1, max_delay_ms: 2, jitter_fraction: 0.0, ..Default::default() });
        let report = foreman.apply_scroll(leaf_scroll("u", vec![apt("flaky")])).unwrap();
        assert!(report.units[0].failures.is_empty());
        assert!(applied_keys(&foreman).contains(&"apt:flaky".to_string()));
    }

    #[test]
    fn round_delay_saturates_at_max_delay() {
        let cfg = crate::config::RetryConfig { base_delay_ms: 100, backoff_multiplier: 10.0, max_delay_ms: 500, jitter_fraction: 0.0, ..Default::default() };
        assert_eq!(super::round_delay(&cfg, 1).as_millis(), 100);
        assert_eq!(super::round_delay(&cfg, 2).as_millis(), 500);
        assert_eq!(super::round_delay(&cfg, 5).as_millis(), 500);
    }

    #[test]
    fn a_never_succeeding_retryable_gives_up_as_retries_exhausted() {
        let reconciler = ScriptedReconciler::new().retryable_always("apt:doomed").ok_default();
        let foreman = foreman_with(reconciler)
            .with_retry_config(crate::config::RetryConfig { max_attempts: 3, base_delay_ms: 1, max_delay_ms: 1, jitter_fraction: 0.0, max_elapsed_ms: 60_000, on_exhaust: crate::config::OnExhaustConfig::Keep, ..Default::default() });
        let report = foreman.apply_scroll(leaf_scroll("u", vec![apt("doomed")])).unwrap();
        assert_eq!(report.units[0].failures.len(), 1);
        assert_eq!(report.units[0].failures[0].class, super::FailClass::RetriesExhausted);
        assert_eq!(report.units[0].failures[0].attempts, 3);
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p golemd --lib round_delay_saturates_at_max_delay`
Expected: compile error — `round_delay` missing; loop has no timing.

- [ ] **Step 3: Implement `round_delay` and wire timing into the loop**

```rust
pub(crate) fn round_delay(retry: &RetryConfig, round: u32) -> Duration {
    let exp = retry.backoff_multiplier.powi((round - 1) as i32);
    let raw = (retry.base_delay_ms as f64 * exp).min(retry.max_delay_ms as f64);
    let jitter = if retry.jitter_fraction > 0.0 {
        let span = raw * retry.jitter_fraction;
        raw + (fastrand::f64() * 2.0 - 1.0) * span
    } else {
        raw
    };
    Duration::from_millis(jitter.max(0.0) as u64)
}
```

In `enact_unit`, capture `let started = std::time::Instant::now();` at the top; between rounds, before re-driving, `std::thread::sleep(round_delay(retry, round));` and add the wall-time guard to the loop's stop condition: also break when `started.elapsed().as_millis() as u64 >= retry.max_elapsed_ms`. Ensure `fastrand` is a dependency (it is in `[workspace.dependencies]`; add `fastrand = { workspace = true }` to `apps/golemd/Cargo.toml` `[dependencies]` if absent).

- [ ] **Step 4: Run**

Run: `cargo test -p golemd --lib a_retryable_glyph_succeeds_within_the_attempt_limit round_delay_saturates_at_max_delay a_never_succeeding_retryable_gives_up_as_retries_exhausted`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/golemd/src/foreman.rs apps/golemd/Cargo.toml
git commit -m "feat(golemd): backoff, jitter, and dual retry limits between rounds

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** document the per-round delay formula, jitter's cross-fleet de-synchronization purpose, and the belt-and-suspenders dual limit (`max_attempts` vs `max_elapsed_ms`, whichever trips first), per ADR 0029 §3.

---

## Task 6: golemd — per-unit `on_exhaust`, rollback scoped by `unit_path`, immediate logging

**Files:**
- Modify: `apps/golemd/src/foreman.rs`
- Test: `apps/golemd/src/foreman.rs` in-crate tests (a unit's `rollback` undoes only its own glyphs; `keep` leaves them; a sibling is untouched either way).

**Interfaces:**
- Consumes: Task 4 `UnitResult`, Task 5 loop, existing `rollback_attempt`.
- Produces: `fn rollback_unit(&self, reconcile_id: u64, unit_path: &[String]) -> Result<()>` — the existing LIFO `rollback_attempt` restricted to steps whose `unit_path == unit_path`; the per-unit settle branch; `warn!`/`error!` at the Failed arms.

- [ ] **Step 1: Write the failing tests**

```rust
    #[test]
    fn a_units_rollback_undoes_only_its_own_glyphs() {
        let reconciler = ScriptedReconciler::new().fatal_on("apt:bad").ok_default();
        let foreman = foreman_with(reconciler);
        let scroll = branch_scroll(
            "host",
            vec![
                leaf_scroll("broken", vec![apt("good-in-broken"), apt("bad")]),
                leaf_scroll("healthy", vec![apt("healthy-pkg")]),
            ],
        );
        let report = foreman.apply_scroll(scroll).unwrap();
        let broken = report.units.iter().find(|u| u.unit_path.last().unwrap() == "broken").unwrap();
        assert_eq!(broken.outcome, super::UnitOutcome::RolledBack);
        assert!(!applied_keys(&foreman).contains(&"apt:good-in-broken".to_string()));
        assert!(applied_keys(&foreman).contains(&"apt:healthy-pkg".to_string()));
    }

    #[test]
    fn a_keep_unit_leaves_its_applied_glyphs() {
        let reconciler = ScriptedReconciler::new().fatal_on("apt:bad").ok_default();
        let foreman = foreman_with(reconciler);
        let leaf = leaf_scroll_with_policy(
            "u",
            scroll_format::Policy { on_exhaust: Some(scroll_format::OnExhaust::Keep), ..Default::default() },
            vec![apt("kept"), apt("bad")],
        );
        let report = foreman.apply_scroll(branch_scroll("host", vec![leaf])).unwrap();
        assert_eq!(report.units[0].outcome, super::UnitOutcome::Partial);
        assert!(applied_keys(&foreman).contains(&"apt:kept".to_string()));
        assert!(report.units[0].failures.iter().all(|f| !f.rolled_back));
    }
```

Add the `leaf_scroll_with_policy` test helper (builds a leaf `Scroll` carrying a `Policy`). `UnitOutcome` arrives in Task 7 — for Task 6, land the rollback/keep behavior and mark the internal `UnitResult.rolled_back`; the `outcome` assertion moves to Task 7 or set `UnitResult` to carry an outcome enum now.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p golemd --lib a_units_rollback_undoes_only_its_own_glyphs`
Expected: compile error — `rollback_unit`, `UnitOutcome`, per-unit settle missing.

- [ ] **Step 3: Implement `rollback_unit` (scoped LIFO reversal)**

Add a scoped variant beside `rollback_attempt` (lines 423–454). It is `rollback_attempt` with the `next_reversible` search filtered to steps whose `unit_path == unit_path`:

```rust
    fn rollback_unit(&self, reconcile_id: u64, unit_path: &[String]) -> Result<()> {
        loop {
            let steps = self.planroom.wal_steps_for(reconcile_id)?;
            let scoped: Vec<WalStep> = steps.into_iter().filter(|s| s.unit_path == unit_path).collect();
            let Some(target) = next_reversible(&scoped).cloned() else { break };
            let cid = applied_cid_of(&target.op, target.action);
            let outcome = Outcome {
                op: target.op.clone(),
                cid,
                inverse: target.inverse.clone().unwrap_or(Inverse::Nothing),
                changed: target.changed.unwrap_or(false),
            };
            let undone = match target.action {
                WalAction::Apply => self.reconciler.reverse(&outcome),
                WalAction::Reverse => self.reconciler.apply(target.op.glyph(), cid).map(|_| ()),
            };
            if let Err(e) = undone {
                warn!(glyph_key = %target.glyph_key, phase = "reverse", ?e, "rollback step failed");
            }
            self.planroom.append_wal_step(
                reconcile_id, target.step_ord, &target.glyph_key, target.action,
                WalStepState::Reversed, &target.op, target.inverse.as_ref(), target.changed,
                &target.unit_path,
            )?;
        }
        Ok(())
    }
```

`next_reversible` currently takes `&[WalStep]` and returns `Option<&WalStep>` — the scoped `Vec<WalStep>` is owned here, so `.cloned()` gives an owned `WalStep`; confirm `next_reversible`'s "latest Done not yet Reversed" logic operates correctly over the filtered slice (it does — it only inspects the passed steps).

- [ ] **Step 4: Add the per-unit `on_exhaust` settle branch and immediate logging**

In `enact_unit`, after `unit_failures` yields the terminal failures, decide the unit's fate on the resolved `retry.on_exhaust`:

```rust
        let failures = self.unit_failures(reconcile_id, unit_path, ops, retry);
        let has_failures = !failures.is_empty();
        let rolled_back = has_failures && retry.on_exhaust == crate::config::OnExhaustConfig::Rollback;
        if rolled_back {
            self.rollback_unit(reconcile_id, unit_path)?;
        }
        Ok(UnitResult {
            unit_path: unit_path.to_vec(),
            failures: failures.into_iter().map(|mut f| { f.rolled_back = rolled_back; f }).collect(),
            outcome: if !has_failures { UnitOutcome::Settled } else if rolled_back { UnitOutcome::RolledBack } else { UnitOutcome::Partial },
        })
```

(Add `rolled_back: bool` to `UnitFailure` and `outcome: UnitOutcome` to `UnitResult`; define `UnitOutcome { Settled, Partial, RolledBack }` here or in Task 7's `report.rs` and re-use.)

Add the immediate log lines at the moment a `Failed` row is written and at give-up. In `enact_apply`/`enact_reverse` Failed arm, add before returning the class:

```rust
                warn!(glyph_key = %op.key(), round = 0, class = "retryable", reason = %"…", "enact failed; will retry");
```

More precisely, thread the round number and class: log `warn!` for a retryable that will retry, `error!` with `class = "retries-exhausted"` when the round loop decides an op is done owing to the limit (emit this in `unit_failures`/at the give-up point), and `error!` with `class = "fatal"` for a fatal. Put the retry/fatal distinction at the classification site (`classify`) and the retries-exhausted `error!` at the loop's give-up decision. Log only `glyph_key` + `reason` (the `EnactError` message) — never contents/secrets.

- [ ] **Step 5: Run**

Run: `cargo test -p golemd --lib a_units_rollback_undoes_only_its_own_glyphs a_keep_unit_leaves_its_applied_glyphs`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/golemd/src/foreman.rs
git commit -m "feat(golemd): per-unit on_exhaust with unit-scoped rollback and live logging

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** document that `rollback` is scoped to the unit's `unit_path` (siblings untouched, ADR 0029 §4 / §Preserving ADR 0020); `keep` leaves the applied set and marks failures `rolled_back = false`; crash recovery of a *whole* attempt still reverses every unit (the `unit_path` narrows only the deliberate per-unit exhaustion rollback); the three immediate log points and their `class` tags (ADR 0029 §2).

---

## Task 7: golemd — the tree-shaped `ReconcileReport` types (`report.rs`)

**Files:**
- Create: `apps/golemd/src/report.rs`
- Modify: `apps/golemd/src/lib.rs`
- Test: `apps/golemd/src/report.rs` in-crate serde tests.

**Interfaces:**
- Consumes: Plan 1 `Revision` (existing `journal::Revision`), Task 4/6 internal `UnitResult`/`UnitFailure`.
- Produces (exactly ADR 0029 §5):
  - `pub struct ReconcileReport { pub revision: Revision, pub outcome: TopOutcome, pub units: Vec<UnitReport> }`
  - `pub struct UnitReport { pub unit_path: Vec<String>, pub outcome: UnitOutcome, pub failures: Vec<GlyphFailure> }`
  - `pub struct GlyphFailure { pub glyph_key: String, pub unit_path: Vec<String>, pub phase: FailPhase, pub class: FailClassReport, pub attempts: u32, pub message: String, pub rolled_back: bool }`
  - `pub enum TopOutcome { Settled, Partial, RolledBack }` (serde `rename_all = "snake_case"` → `"settled"`/`"partial"`/`"rolled_back"`)
  - `pub enum UnitOutcome { Settled, Partial, RolledBack }` (same rename)
  - `pub enum FailPhase { Enact, Reverse, Recovery }` (rename → `"enact"`/`"reverse"`/`"recovery"`)
  - `pub enum FailClassReport { Fatal, RetriesExhausted }` (rename → `"fatal"`/`"retries-exhausted"` via `rename` on each variant)
  - `impl ReconcileReport { pub fn roll_up(revision: Revision, units: Vec<UnitReport>) -> ReconcileReport }` — top `outcome` is `Settled` iff every unit settled; else `Partial` if any unit kept a partial set; else `RolledBack`.

- [ ] **Step 1: Declare the module and write the failing serde test**

In `apps/golemd/src/lib.rs`, add `pub mod report;`. Create `apps/golemd/src/report.rs` with the types above and:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_settled_unit_serializes_with_snake_case_outcome() {
        let unit = UnitReport { unit_path: vec!["h".into(), "u".into()], outcome: UnitOutcome::Settled, failures: vec![] };
        let json = serde_json::to_value(&unit).unwrap();
        assert_eq!(json["outcome"], "settled");
    }

    #[test]
    fn a_glyph_failure_class_renders_retries_exhausted() {
        let f = GlyphFailure {
            glyph_key: "apt:x".into(),
            unit_path: vec!["h".into()],
            phase: FailPhase::Enact,
            class: FailClassReport::RetriesExhausted,
            attempts: 3,
            message: "mirror down".into(),
            rolled_back: false,
        };
        let json = serde_json::to_value(&f).unwrap();
        assert_eq!(json["class"], "retries-exhausted");
        assert_eq!(json["phase"], "enact");
    }

    #[test]
    fn roll_up_is_settled_only_when_all_units_settle() {
        let rev = crate::journal::Revision { id: 2, created_at: chrono::Utc::now(), kind: crate::journal::RevisionKind::Reconcile, scroll_content_id: None, outcomes: vec![] };
        let settled = UnitReport { unit_path: vec!["h".into(), "a".into()], outcome: UnitOutcome::Settled, failures: vec![] };
        let partial = UnitReport { unit_path: vec!["h".into(), "b".into()], outcome: UnitOutcome::Partial, failures: vec![] };
        let all_settled = ReconcileReport::roll_up(rev.clone(), vec![settled.clone()]);
        assert_eq!(all_settled.outcome, TopOutcome::Settled);
        let mixed = ReconcileReport::roll_up(rev, vec![settled, partial]);
        assert_eq!(mixed.outcome, TopOutcome::Partial);
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p golemd --lib report::tests`
Expected: compile error — the types do not exist.

- [ ] **Step 3: Implement the report types**

```rust
use serde::Serialize;

use crate::journal::Revision;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TopOutcome {
    Settled,
    Partial,
    RolledBack,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitOutcome {
    Settled,
    Partial,
    RolledBack,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailPhase {
    Enact,
    Reverse,
    Recovery,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum FailClassReport {
    #[serde(rename = "fatal")]
    Fatal,
    #[serde(rename = "retries-exhausted")]
    RetriesExhausted,
}

#[derive(Debug, Clone, Serialize)]
pub struct GlyphFailure {
    pub glyph_key: String,
    pub unit_path: Vec<String>,
    pub phase: FailPhase,
    pub class: FailClassReport,
    pub attempts: u32,
    pub message: String,
    pub rolled_back: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnitReport {
    pub unit_path: Vec<String>,
    pub outcome: UnitOutcome,
    pub failures: Vec<GlyphFailure>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReconcileReport {
    pub revision: Revision,
    pub outcome: TopOutcome,
    pub units: Vec<UnitReport>,
}

impl ReconcileReport {
    pub fn roll_up(revision: Revision, units: Vec<UnitReport>) -> ReconcileReport {
        let outcome = if units.iter().all(|u| u.outcome == UnitOutcome::Settled) {
            TopOutcome::Settled
        } else if units.iter().any(|u| u.outcome == UnitOutcome::Partial) {
            TopOutcome::Partial
        } else {
            TopOutcome::RolledBack
        };
        ReconcileReport { revision, outcome, units }
    }
}
```

Confirm `journal::Revision` derives `Serialize` and `Clone` (it is already serialized by `http.rs` today, so it does).

- [ ] **Step 4: Run**

Run: `cargo test -p golemd --lib report::tests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/golemd/src/report.rs apps/golemd/src/lib.rs
git commit -m "feat(golemd): tree-shaped ReconcileReport types

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** document the report shape mirrors the host scroll (ADR 0029 §5 / ADR 0031 §6); each field's meaning; the roll-up rule; the exact JSON tags (`retries-exhausted`, `rolled_back`, snake_case outcomes) the fleet CLI parses.

---

## Task 8: golemd — `reconcile`/`apply_manifest` return a `ReconcileReport`; typed `ForemanError`; HTTP 200-in-band + structured errors

**Files:**
- Modify: `apps/golemd/src/foreman.rs` (`reconcile`, `apply_manifest`, typed error)
- Modify: `apps/golemd/src/http.rs`
- Test: `apps/golemd/src/foreman.rs` in-crate + `apps/golemd/tests/report_api.rs` (new) or extend an existing integration test.

**Interfaces:**
- Consumes: Tasks 3–7 (`resolve_retry`, `enact_unit`, `UnitResult`, `report::*`).
- Produces:
  - `Foreman::apply_manifest(&self, bytes: &[u8]) -> Result<ReconcileReport, ForemanError>`
  - `pub enum ForemanError { WalUnreadable { detail: String }, ManifestUndecodable { detail: String }, Internal(anyhow::Error) }` with `fn kind(&self) -> &'static str` and `fn message(&self) -> String`.
  - `reconcile(&self, desired: SelectedScroll) -> Result<ReconcileReport, ForemanError>` — walks `desired.scroll.leaf_units()` in source order, resolves each unit's `RetryConfig` via `resolve_retry(&self.retry, &unit.policy_chain)`, plans+enacts each unit, collects `UnitReport`s, settles, projects the `Revision`, and rolls up.

- [ ] **Step 1: Write the failing tests**

Extend the foreman tests (report shape) and add an HTTP integration test. In-crate:

```rust
    #[test]
    fn apply_manifest_returns_a_report_with_units_in_source_order() {
        let reconciler = ScriptedReconciler::new().ok_default();
        let foreman = foreman_with(reconciler);
        let scroll = branch_scroll("host", vec![leaf_scroll("a", vec![apt("one")]), leaf_scroll("b", vec![apt("two")])]);
        let report = foreman.apply_scroll(scroll).unwrap();
        assert_eq!(report.outcome, crate::report::TopOutcome::Settled);
        let names: Vec<String> = report.units.iter().map(|u| u.unit_path.last().unwrap().clone()).collect();
        assert_eq!(names, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn an_undecodable_manifest_is_a_typed_error() {
        let foreman = foreman_with(ScriptedReconciler::new().ok_default());
        match foreman.apply_manifest(b"not a manifest") {
            Err(e) => assert_eq!(e.kind(), "manifest-undecodable"),
            Ok(_) => panic!("expected a typed error"),
        }
    }
```

Create `apps/golemd/tests/report_api.rs` (integration, over the HTTP router) modeled on the existing integration tests — it POSTs a manifest with one failing unit and asserts the response is **HTTP 200** with a JSON body whose top `outcome` is `rolled_back` and `units[0].failures[0].class` is present. (Read an existing `apps/golemd/tests/*.rs` for how they spin up the router / call the foreman; reuse that harness.)

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p golemd --lib apply_manifest_returns_a_report_with_units_in_source_order`
Expected: compile error — `apply_manifest` returns `Revision`, no `ForemanError`, no per-unit walk.

- [ ] **Step 3: Define `ForemanError`**

In `apps/golemd/src/foreman.rs`:

```rust
#[derive(Debug)]
pub enum ForemanError {
    WalUnreadable { detail: String },
    ManifestUndecodable { detail: String },
    Internal(anyhow::Error),
}

impl ForemanError {
    pub fn kind(&self) -> &'static str {
        match self {
            ForemanError::WalUnreadable { .. } => "wal-unreadable",
            ForemanError::ManifestUndecodable { .. } => "manifest-undecodable",
            ForemanError::Internal(_) => "internal",
        }
    }

    pub fn message(&self) -> String {
        match self {
            ForemanError::WalUnreadable { .. } => "golemd couldn't read its write-ahead log; it may be from an incompatible golemd version. Run `fleet reset` on this host to start from a clean state.".to_string(),
            ForemanError::ManifestUndecodable { detail } => format!("golemd couldn't decode the manifest: {detail}"),
            ForemanError::Internal(e) => format!("{e:#}"),
        }
    }
}

impl std::fmt::Display for ForemanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for ForemanError {}
```

- [ ] **Step 4: Rewrite `reconcile` and `apply_manifest`**

`apply_manifest` (lines 87–101): decode via `from_bytes`, mapping the error to `ForemanError::ManifestUndecodable { detail }`; select the host scroll; call `reconcile`. `reconcile` (lines 121–147): keep the write-lock, recover, and unsettled-attempt gate (map WAL-read failures to `ForemanError::WalUnreadable`), then:

```rust
    fn reconcile(&self, desired: SelectedScroll) -> Result<ReconcileReport, ForemanError> {
        let _w = self.write.lock().unwrap();
        self.recover_locked().map_err(|e| ForemanError::WalUnreadable { detail: e.to_string() })?;
        let steps = self.planroom.wal_steps().map_err(|e| ForemanError::WalUnreadable { detail: e.to_string() })?;
        if let Some(attempt) = self.planroom.latest_attempt().map_err(|e| ForemanError::WalUnreadable { detail: e.to_string() })? {
            if !attempt.phase.is_settled() {
                return Err(ForemanError::Internal(anyhow::anyhow!("reconcile {} is unsettled ({:?}); refusing new manifest", attempt.reconcile_id, attempt.phase)));
            }
        }
        let prior = applied_outcomes(&steps);
        let attempt = self.planroom.open_attempt(Some(desired.content_id)).map_err(|e| ForemanError::Internal(e.into()))?;
        self.planroom.set_attempt_phase(attempt.reconcile_id, AttemptPhase::Enacting).map_err(|e| ForemanError::Internal(e.into()))?;

        let mut unit_reports = Vec::new();
        for unit in desired.scroll.leaf_units() {
            let effective = resolve_retry(&self.retry, &unit.policy_chain);
            let ops = plan(&prior, &leaf_as_scroll(&unit));
            let result = self
                .enact_unit(attempt.reconcile_id, &ops, &prior, &unit.path, &effective)
                .map_err(ForemanError::Internal)?;
            unit_reports.push(unit_report_from(result));
        }

        let revision = self.settle(attempt.reconcile_id, &desired).map_err(|e| ForemanError::Internal(e.into()))?;
        Ok(ReconcileReport::roll_up(revision, unit_reports))
    }
```

Two helpers: `leaf_as_scroll(unit: &LeafUnit) -> Scroll` builds a leaf `Scroll { name: unit.path.last().cloned().unwrap_or_default(), policy: None, contents: Contents::Glyphs(unit.glyphs.to_vec()) }` so `plan` diffs this unit's glyphs against the whole prior set (prior is the full applied set; the diff naturally yields Installs/Replaces for this unit's glyphs). **Note (flagged):** removes for *vanished* units are not produced by per-unit `plan` calls over present units — a vanished unit's glyphs are in `prior` but in no present leaf. Handle vanished-unit removes explicitly: after the per-unit loop, compute `plan(&prior, &desired.scroll)` (the whole-scroll diff over `all_glyphs`), take only its `Remove` ops, group them under the surviving parent's `unit_path` (ADR 0031 §4: nearest still-present ancestor), and enact them as one extra "removes" unit under the parent policy. Implement this as a final pass: `let removes: Vec<GlyphOp> = plan(&prior, &desired.scroll).into_iter().filter(|o| matches!(o, GlyphOp::Remove{..})).collect();` then enact under a `unit_path` of the host root (Plan-2-acceptable approximation is the host root; ADR asks for the nearest surviving parent — see the flagged interpretation in the report).

`unit_report_from(result: UnitResult) -> UnitReport` maps the internal outcome/failures into `report::UnitReport`/`report::GlyphFailure` (mapping `FailClass::Fatal → FailClassReport::Fatal`, `RetriesExhausted → RetriesExhausted`; `Phase::Enact → FailPhase::Enact`, `Reverse → FailPhase::Reverse`).

- [ ] **Step 5: Update `http.rs` — 200-with-report and structured `ApiError`**

Replace `ApiError` (lines 103–121) so its body is JSON `{ kind, message }`:

```rust
#[derive(serde::Serialize)]
struct ApiError {
    #[serde(skip)]
    status: StatusCode,
    kind: String,
    message: String,
}

impl ApiError {
    fn from_foreman(e: crate::foreman::ForemanError) -> Self {
        let status = match e {
            crate::foreman::ForemanError::WalUnreadable { .. }
            | crate::foreman::ForemanError::ManifestUndecodable { .. }
            | crate::foreman::ForemanError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        ApiError { status, kind: e.kind().to_string(), message: e.message() }
    }
    fn not_found(message: String) -> Self {
        ApiError { status: StatusCode::NOT_FOUND, kind: "not-found".to_string(), message }
    }
    fn internal(e: anyhow::Error) -> Self {
        ApiError { status: StatusCode::INTERNAL_SERVER_ERROR, kind: "internal".to_string(), message: format!("{e:#}") }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (self.status, Json(self)).into_response()
    }
}
```

`apply_manifest` route (lines 63–69): call the foreman directly (it now returns `Result<ReconcileReport, ForemanError>`), map the `Ok(report)` to `Json(report)` at **200** and `Err(e)` to `ApiError::from_foreman(e)`. The `blocking` helper's bound is `FnOnce(&Foreman) -> anyhow::Result<T>`; add a second helper or change the route to a dedicated blocking call that returns `Result<ReconcileReport, ForemanError>` and maps it. Concretely:

```rust
async fn apply_manifest(
    AxState(s): AxState<AppState>,
    body: Bytes,
) -> Result<impl IntoResponse, ApiError> {
    let bytes = body.to_vec();
    let foreman = s.foreman.clone();
    let report = tokio::task::spawn_blocking(move || foreman.apply_manifest(&bytes))
        .await
        .map_err(|e| ApiError::internal(anyhow::anyhow!("task join: {e}")))?
        .map_err(ApiError::from_foreman)?;
    Ok(Json(report))
}
```

The read routes (`state`/`revisions`/`revision`/`status`) keep using the existing `blocking` helper and `ApiError::internal`/`not_found`.

- [ ] **Step 6: Run**

Run: `cargo test -p golemd --lib apply_manifest_returns_a_report_with_units_in_source_order an_undecodable_manifest_is_a_typed_error`
Run: `cargo test -p golemd --test report_api`
Expected: PASS. Also fix any `golemctl` caller that expected a `Revision` body from `/manifest` — `golemctl apply` prints the response; check `rg -n 'apply|manifest|revision' apps/golemctl/src` and update its parsing to the report shape (or leave it printing raw JSON if it does).

- [ ] **Step 7: Commit**

```bash
git add apps/golemd/src/foreman.rs apps/golemd/src/http.rs apps/golemd/tests/report_api.rs
git commit -m "feat(golemd): apply returns a tree ReconcileReport; typed structured errors

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** document 200-in-band for partial/rolled-back reconciles (a successful RPC reporting goal failures, not a transport error — ADR 0029 §5); non-2xx reserved for daemon/transport errors, now carrying `{ kind, message }`; the `WalUnreadable`/`ManifestUndecodable` actionable messages; the per-unit `plan` + vanished-unit removes pass and its policy scoping (flagged interpretation).

---

## Task 9: golemd — removes in reverse source order

**Files:**
- Modify: `apps/golemd/src/reconcile.rs`
- Test: `apps/golemd/src/reconcile.rs` in-crate tests.

**Interfaces:**
- Consumes: nothing new.
- Produces: `plan` emits `Remove` ops in **reverse** of the order they appear in `prior` (reverse-of-apply), so teardown unwinds opposite to setup (ADR 0029 §6).

- [ ] **Step 1: Write the failing test**

In `apps/golemd/src/reconcile.rs` tests:

```rust
    #[test]
    fn removes_come_out_in_reverse_prior_order() {
        let prior = vec![applied(apt("first")), applied(apt("second")), applied(apt("third"))];
        let ops = plan(&prior, &scroll(vec![]));
        assert_eq!(
            ops,
            vec![
                GlyphOp::Remove { cid: glyph_content_id(&apt("third")), glyph: apt("third") },
                GlyphOp::Remove { cid: glyph_content_id(&apt("second")), glyph: apt("second") },
                GlyphOp::Remove { cid: glyph_content_id(&apt("first")), glyph: apt("first") },
            ]
        );
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p golemd --lib removes_come_out_in_reverse_prior_order`
Expected: FAIL — current order is forward.

- [ ] **Step 3: Reverse the removes pass**

In `apps/golemd/src/reconcile.rs`, change the removes loop (lines 44–51) to iterate `prior` in reverse:

```rust
    for prev in prior.iter().rev() {
        if !seen.contains(&prev.op.key()) {
            ops.push(GlyphOp::Remove {
                cid: prev.cid,
                glyph: prev.op.glyph().clone(),
            });
        }
    }
```

- [ ] **Step 4: Update the existing ordering test**

The existing `installs_precede_removes_and_follow_desired_order` (lines 120–132) asserts a single remove, so it is unaffected. Confirm it still passes.

- [ ] **Step 5: Run**

Run: `cargo test -p golemd --lib removes_come_out_in_reverse_prior_order installs_precede_removes_and_follow_desired_order`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/golemd/src/reconcile.rs
git commit -m "feat(golemd): tear down removes in reverse source order

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** document that removes now unwind in reverse-of-apply order (safe teardown of dependent resources), the one ordering refinement of ADR 0029 §6; install/replace order is unchanged.

---

## Task 10: docs — the ordering-contract note

**Files:**
- Modify: `apps/emet/CLAUDE.md` (add the author-facing ordering-contract note)
- Test: none (prose). This task is included because ADR 0029 §6 explicitly requires the contract be written down; it is documenter-owned prose, so this task only creates the anchor.

**Interfaces:** none.

- [ ] **Step 1: Add the ordering-contract note**

Append to `apps/emet/CLAUDE.md`, under a new heading, the ADR 0029 §6 note verbatim (source order — units then glyphs, author-controlled, no DAG; removes in reverse for safe teardown), cross-linked to ADR 0029 and ADR 0031. Since this is documenter-owned prose (not code), the implementer adds the heading and a one-line pointer to the ADR; the documenter fills the full wording.

- [ ] **Step 2: Commit**

```bash
git add apps/emet/CLAUDE.md
git commit -m "docs(emet): anchor the apply-order contract note

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** documenter writes the full ordering-contract note (units first then glyphs, source order, author-controlled, no dependency DAG across or within units, removes in reverse); mention `golemd.toml`/`--config` in `QUICKSTART.md`.

---

## Task 11: fleet — render the per-unit report, failures block, and typed errors

**Files:**
- Modify: `apps/fleet/cli.py`
- Create: `apps/fleet/tests/test_apply_render.py`
- Test: the new file (mock the report shapes).

**Interfaces:**
- Consumes: the golemd report JSON (Task 7/8 shape) and the typed error body `{ kind, message }` (Task 8). Reuses `_render_revision` (cli.py:206), `_glyph_desc` (cli.py:169), `_op_parts` (cli.py:196), `_cid_hex`/`_cid_short` (cli.py:150/163).
- Produces: `apply` renders, on 200, the settled revision (via `_render_revision`) then each `UnitReport` under its name-path colored by unit `outcome`, with a red failures block per `GlyphFailure`; on a typed non-2xx, prints the `message` (not `response.text`).

- [ ] **Step 1: Write the failing tests**

Create `apps/fleet/tests/test_apply_render.py` (`unittest`-style to match the existing suite):

```python
import io
import unittest
from unittest import mock

from rich.console import Console


class RenderReportTests(unittest.TestCase):
    def _render(self, report):
        from fleet import cli
        buf = io.StringIO()
        console = Console(file=buf, force_terminal=False, no_color=True, width=200)
        with mock.patch.object(cli, "console", console):
            cli._render_report("vm-1", report)
        return buf.getvalue()

    def test_settled_unit_renders_its_path(self):
        report = {
            "revision": {"id": 3, "kind": "reconcile", "scroll_content_id": None, "outcomes": []},
            "outcome": "settled",
            "units": [{"unit_path": ["host", "base"], "outcome": "settled", "failures": []}],
        }
        out = self._render(report)
        self.assertIn("host / base", out)
        self.assertIn("settled", out)

    def test_failure_line_shows_class_attempts_and_message(self):
        report = {
            "revision": {"id": 4, "kind": "reconcile", "scroll_content_id": None, "outcomes": []},
            "outcome": "rolled_back",
            "units": [{
                "unit_path": ["host", "app"],
                "outcome": "rolled_back",
                "failures": [{
                    "glyph_key": "apt:nginx",
                    "unit_path": ["host", "app"],
                    "phase": "enact",
                    "class": "retries-exhausted",
                    "attempts": 5,
                    "message": "mirror down",
                    "rolled_back": True,
                }],
            }],
        }
        out = self._render(report)
        self.assertIn("apt nginx", out)
        self.assertIn("retries-exhausted", out)
        self.assertIn("after 5 tries", out)
        self.assertIn("mirror down", out)

    def test_typed_error_prints_message_not_raw_text(self):
        from fleet import cli
        buf = io.StringIO()
        console = Console(file=buf, force_terminal=False, no_color=True, width=200)
        with mock.patch.object(cli, "console", console):
            cli._render_apply_error("vm-1", 500, {"kind": "wal-unreadable", "message": "Run `fleet reset`"})
        out = buf.getvalue()
        self.assertIn("Run `fleet reset`", out)
        self.assertIn("wal-unreadable", out)
```

- [ ] **Step 2: Run to verify failure**

Run: from repo root, `PYTHONPATH=apps python -m unittest apps.fleet.tests.test_apply_render`
Expected: FAIL — `_render_report` / `_render_apply_error` do not exist.

- [ ] **Step 3: Implement the renderers**

Add to `apps/fleet/cli.py`, reusing existing helpers:

```python
_UNIT_COLOR = {"settled": "green", "partial": "yellow", "rolled_back": "red"}


def _render_report(name: str, report: dict) -> None:
    revision = report.get("revision") or {}
    _render_revision(name, revision)
    top = report.get("outcome", "")
    color = _UNIT_COLOR.get(top, "white")
    console.print(f"  [{color}]apply {top}[/{color}]")
    for unit in report.get("units") or []:
        path = " / ".join(unit.get("unit_path") or [])
        outcome = unit.get("outcome", "")
        ucolor = _UNIT_COLOR.get(outcome, "white")
        console.print(f"    [{ucolor}]{path}: {outcome}[/{ucolor}]")
        for failure in unit.get("failures") or []:
            desc = _glyph_desc({_glyph_kind_from_key(failure.get('glyph_key')): {}}) if False else failure.get("glyph_key")
            cls = failure.get("class", "")
            attempts = failure.get("attempts", 0)
            message = failure.get("message", "")
            console.print(
                f"      [red]✗ {failure.get('glyph_key')}  {cls} after {attempts} tries — {message}[/red]"
            )


def _render_apply_error(name: str, status: int, body: dict) -> None:
    kind = body.get("kind", "error")
    message = body.get("message", "")
    console.print(f"  [red]{name}: {kind} (HTTP {status})[/red]\n  {message}")
```

(The `_glyph_desc` reuse for a failure is optional — a `GlyphFailure` carries only `glyph_key`, not the full glyph, so the failure line uses `glyph_key` directly. Drop the dead `if False` expression; the final line renders `glyph_key`. Keep `_glyph_desc` for the revision's op table via `_op_parts`, unchanged.)

- [ ] **Step 4: Rewire `apply` to use the report renderers**

In `apply` (cli.py:224), replace the response handling (lines 243–253). On non-200, attempt to parse a typed body and call `_render_apply_error`; on 200, call `_render_report`:

```python
        response = golemd_client.apply_manifest(record, manifest)
        if response.status_code != 200:
            try:
                body = response.json()
            except ValueError:
                body = {"kind": "error", "message": response.text}
            _render_apply_error(record.name, response.status_code, body)
            continue
        report = response.json()
        if raw:
            console.print(f"  [green]{record.name}: revision {report.get('revision', {}).get('id')}[/green]")
            console.print_json(json.dumps(report))
        else:
            _render_report(record.name, report)
```

- [ ] **Step 5: Run**

Run: from repo root, `PYTHONPATH=apps python -m unittest apps.fleet.tests.test_apply_render`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/fleet/cli.py apps/fleet/tests/test_apply_render.py
git commit -m "feat(fleet): render per-unit report blocks, failures, and typed errors

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** document the render layout (revision summary, top outcome header, per-unit blocks under name-paths colored by outcome, red failure lines `✗ <glyph_key>  <class> after <n> tries — <message>`), and that a typed non-2xx prints the actionable `message` not `response.text`; note the CLI now parses one typed body on the 200 path (ADR 0029 §5).

---

## Task 12: whole-workspace green + acceptance

**Files:** whole workspace.

**Interfaces:** all prior tasks.

- [ ] **Step 1: Full workspace test**

Run: `cargo test --workspace`
Expected: PASS across all crates. Fix any straggler `golemctl` caller of `/manifest` that assumed a `Revision` body.

- [ ] **Step 2: Fleet tests**

Run: from repo root, `PYTHONPATH=apps python -m unittest discover apps/fleet/tests`
Expected: PASS.

- [ ] **Step 3: Acceptance — a mixed-outcome fleet applies through the fake reconciler with a 200 report**

Build and run a golemd (fake reconciler) and apply a nested program where one leaf's package is scripted to fail — verify the HTTP response is 200 and the body's top `outcome` is `partial`/`rolled_back` while a sibling unit is `settled`. Since the fake reconciler does not fail, this acceptance is covered by the `apps/golemd/tests/report_api.rs` integration test (Task 8) which uses a scripted failing reconciler; re-run it:

Run: `cargo test -p golemd --test report_api`
Expected: PASS — 200 with a tree report, sibling isolation visible.

- [ ] **Step 4: Release build**

Run: `cargo build --release -p golemd -p golemctl -p emet`
Expected: clean.

- [ ] **Step 5: Commit any straggler fixes**

```bash
git add <files touched in Step 1>
git commit -m "chore(golemd): finish report-shape sweep across the workspace

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

(Skip if no changes.)

**Doc backlog:** update `QUICKSTART.md` to mention `golemd.toml`/`--config`, that `apply` returns a per-unit report, and that partial/rolled-back reconciles are HTTP 200 with an in-band report.

---

## Self-Review

**1. Spec coverage (ADR 0029, revised):**

- §1 best-effort per-unit round loop, WAL fold, one spine (delete `attempt`/`attempt_reverse`) → Tasks 4–5. ✓
- §2 immediate logging at Failed arms (retryable-will-retry `warn!`, retries-exhausted/fatal `error!`) → Task 6 Step 4. ✓
- §3 `golemd.toml [retry]` config, `--config`, `with_retry_config`, cascade override → Tasks 1–3. ✓
- §4 `on_exhaust` rollback (scoped by `unit_path`) vs keep, default rollback, sibling isolation → Task 6. ✓
- §5 tree-shaped `ReconcileReport`/`UnitReport`/`GlyphFailure`, exact JSON tags, HTTP 200 in-band, typed `{ kind, message }` for `WalUnreadable`/`ManifestUndecodable` → Tasks 7–8. ✓
- §6 ordering-contract note + removes in reverse → Tasks 9–10. ✓
- §Preserving ADR 0020: bracketing intact (every op still `Intended`→`Done`/`Failed`), recovery reused, `rollback` scoped by `unit_path`, whole-attempt crash recovery unchanged → Tasks 4/6 (bracketing kept in `enact_apply`/`enact_reverse`; `rollback_unit` reuses `next_reversible`/`applied_cid_of`). ✓

ADR 0031 dependencies (`leaf_units`, `policy_chain`, `unit_path` column, `Policy`/`OnExhaust` types) are all consumed from Plan 1 and named in the "Interfaces consumed from Plan 1" block. ✓

**2. Placeholder scan:** No `TODO`/`handle edge cases`/`add validation`. Two spots carry *explicit implementation choices* rather than placeholders: Task 4's `StepClass`/in-memory-class-tracking (spelled out, with the reason the class is not in the WAL — it never was) and Task 8's vanished-unit-removes pass (spelled out, with the flagged approximation on the parent `unit_path`). Both name the exact code and the tradeoff; neither is a "fill in later." Every code step shows code.

**3. Type consistency:** `RetryConfig` (7 fields + `OnExhaustConfig`) is identical across Tasks 1–3, 5. `resolve_retry(&RetryConfig, &[&Policy]) -> RetryConfig` (Task 3) is called with `&self.retry` + `unit.policy_chain` in Task 8. `enact_unit(reconcile_id, ops, prior, unit_path, retry) -> Result<UnitResult>` is consistent Tasks 4–6/8. `UnitResult { unit_path, failures, outcome }` / `UnitFailure { …, rolled_back }` map into `report::UnitReport`/`GlyphFailure` (Task 7) via `unit_report_from` (Task 8). The two class enums are reconciled explicitly (Task 4 Step 4 note: in-flight `RetryClass{Fatal,Retryable}` vs terminal `FailClass{Fatal,RetriesExhausted}` vs report `FailClassReport{Fatal,RetriesExhausted}`). `apply_manifest -> Result<ReconcileReport, ForemanError>` and `ForemanError::{kind,message}` are consistent across Tasks 8/11 and the HTTP body. `append_wal_step(..., unit_path)` and `WalStep.unit_path` are consumed exactly as Plan 1 produces them.

**Flagged interpretations (see the plan-level report — not silently decided):** (a) the vanished-unit removes' `unit_path` scoping ("nearest surviving parent" per ADR 0031 §4) is approximated to the host root in Task 8, pending Dr. Dub's call on how far to walk the surviving ancestor chain; (b) the retryable/fatal *class* lives in-memory for the live loop (the WAL never stored it), while the WAL remains the crash-recovery truth — consistent with ADR 0029 §Preserving ADR 0020 but worth confirming.
