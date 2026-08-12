# Two-column `golemctl plan` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `golemctl plan --against-host` shows, beside today's diff against golem's journal, a second diff against what is actually on the host right now — so an operator can see before touching anything that a host changed out of band (by Ansible) already matches the desired manifest.

**Architecture:** A new defaulted `Reconciler::observe(&[GlyphOp]) -> Observations` port method. A **verdict** crosses the port — one of four enum values per glyph key — never host state, because the comparison needs the host's passwd database and the fleet key, both of which live inside the adapter. `reconcile::plan` is untouched and stays pure; the probe runs beside it and the report builder joins the two as data.

**Tech Stack:** Rust (axum, serde, clap, reqwest, tempfile, nix), Python (typer) for `apps/fleet`, Astro/Starlight MDX for `sites/website`.

**Binding design document:** `/tmp/claude-1000/-home-lakin-personal-repos-dull-ca-golem/a2ac2e16-4245-4936-b89f-fd86e1e81c6e/scratchpad/design-decisions.md` — read it before Task 1. It carries the reasoning this plan only summarizes, and the per-kind semantics table an implementer must not improvise.

## Global Constraints

- Branch is `lakin/plan-live-host-diff`. Never push. Never rewrite published history.
- **Today's journal-based plan keeps its exact current semantics** and stays the default path. With `--against-host` off, no host is read, no key is needed, and `POST /plan`'s response body is **byte-identical** to today.
- **Default terminal output is byte-identical to today with exactly one documented exception:** the enrollment hint line of Task 7b, which appears only when `against_revision` is `None`. That is a discoverability affordance for an opt-in flag, not a semantic change — the diff itself is untouched. It changes **exactly one** existing golden, the no-prior-revision case at `apps/golemctl/src/plan.rs:1070-1078`. Every other golden in `plan.rs:798-1360` must pass **unmodified**; if an implementer finds itself editing a second one, stop and report — the default path has drifted.
- **Do not change the wire format.** No `scroll-format` field or variant reordering. No `format_version` bump. The manifest is not involved in this feature.
- **Do not run `cargo clean` and do not delete anything under `target/`.** Another agent is running concurrently against `target/release/emetc`.
- Leave `lib/`, `examples/`, `docs/` and `QUICKSTART.md` readable and structurally unchanged — another agent reads them as reference. Adding a `docs/adr/` file is fine; restructuring is not.
- **No new non-defaulted `Reconciler` trait method.** There are 32 `impl Reconciler` in the workspace; a required method breaks all of them.
- **No reality-diff field ever carries host state, only a verdict.** This is the security invariant of the feature. A wire field holding file contents, a mode, a uid, or a dpkg status line is a defect, not a nicety.
- `plan` always exits 0. An unknown observation is not a diff and must not change the exit code.
- Zero comments from `lw:implementer`; `lw:documenter` owns every comment and doc afterward.
- Build/test through devenv: `direnv exec . cargo test -p golemd -p golemctl` from the repo root.

---

### Task 1: The observation vocabulary

**Files:**
- Create: `apps/golemd/src/observe.rs`
- Modify: `apps/golemd/src/lib.rs` (add `pub mod observe;` to the module list, alphabetical among the existing `pub mod` lines)
- Test: `apps/golemd/src/observe.rs` (`#[cfg(test)] mod tests` at the bottom, matching the crate's in-file test convention)

**Interfaces:**
- Consumes: `crate::journal::GlyphOp` is *not* needed here — this module imports only `std`. Keep it that way.
- Produces:
  ```rust
  pub enum Observation { Realized, Divergent, Absent, Unknown(Unknowable) }
  pub enum Unknowable { Sealed, Unreadable, NotModelled }
  pub struct Observations(std::collections::BTreeMap<String, Observation>);
  impl Observations {
      pub fn record(&mut self, key: String, observation: Observation);
      pub fn get(&self, key: &str) -> Observation;
      pub fn is_empty(&self) -> bool;
  }
  impl FromIterator<(String, Observation)> for Observations { … }
  ```
  Derives: `Observation` and `Unknowable` are `#[derive(Debug, Clone, Copy, PartialEq, Eq)]`; `Observations` is `#[derive(Debug, Clone, Default)]`.

**The one behavior that matters:** `get` is **total** — it returns `Observation::Unknown(Unknowable::NotModelled)` for a key the probe never recorded, never an `Option`. A partial probe must degrade to honest ignorance, not a missing row and not a panic.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unrecorded_key_reads_as_not_modelled() {
        let observations = Observations::default();
        assert_eq!(
            observations.get("apt:nginx"),
            Observation::Unknown(Unknowable::NotModelled)
        );
    }

    #[test]
    fn a_recorded_key_reads_back_verbatim() {
        let mut observations = Observations::default();
        observations.record("apt:nginx".to_string(), Observation::Realized);
        assert_eq!(observations.get("apt:nginx"), Observation::Realized);
    }

    #[test]
    fn recording_a_key_twice_keeps_the_last_verdict() {
        let mut observations = Observations::default();
        observations.record("file:/etc/motd".to_string(), Observation::Absent);
        observations.record("file:/etc/motd".to_string(), Observation::Divergent);
        assert_eq!(observations.get("file:/etc/motd"), Observation::Divergent);
    }

    #[test]
    fn a_default_observations_is_empty_and_a_recorded_one_is_not() {
        let mut observations = Observations::default();
        assert!(observations.is_empty());
        observations.record("apt:curl".to_string(), Observation::Realized);
        assert!(!observations.is_empty());
    }

    #[test]
    fn collecting_pairs_builds_the_same_map() {
        let observations: Observations = vec![
            ("apt:nginx".to_string(), Observation::Realized),
            ("apt:curl".to_string(), Observation::Absent),
        ]
        .into_iter()
        .collect();
        assert_eq!(observations.get("apt:nginx"), Observation::Realized);
        assert_eq!(observations.get("apt:curl"), Observation::Absent);
        assert_eq!(
            observations.get("apt:jq"),
            Observation::Unknown(Unknowable::NotModelled)
        );
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `direnv exec . cargo test -p golemd observe::`
Expected: FAIL — `unresolved module` / `cannot find type Observations`.

- [ ] **Step 3: Write the minimal implementation**

The four types and three methods above, plus the `FromIterator` impl. No comments — `lw:documenter` adds them in the documentation pass.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `direnv exec . cargo test -p golemd observe::`
Expected: PASS, 5 tests.

Then run the whole crate to prove nothing regressed:
Run: `direnv exec . cargo test -p golemd`
Expected: PASS, all 257 pre-existing tests still green.

- [ ] **Step 5: Commit** (via `lw:historian`)

---

### Task 2: The `observe` port method and its forwarders

**Files:**
- Modify: `apps/golemd/src/reconciler.rs` — add the defaulted trait method after `prepare` (~`:74`); add forwarding to `impl Reconciler for Arc<R>` (`:119`), `impl Reconciler for Box<R>` (`:148`), and `impl Reconciler for PanicCatching<R>` (`:207`)
- Test: `apps/golemd/src/reconciler.rs` (new `#[cfg(test)] mod tests` — this file currently has none)

**Interfaces:**
- Consumes: `crate::observe::{Observation, Observations, Unknowable}` from Task 1; `crate::journal::GlyphOp`.
- Produces:
  ```rust
  // on trait Reconciler, DEFAULTED
  fn observe(&self, _ops: &[GlyphOp]) -> Observations {
      Observations::default()
  }
  ```
  Note the return type: **no `Result`**. `observe` is infallible by contract, like `diagnose`. A probe that cannot answer records `Unknown` for that glyph and keeps going.

**The trap this task exists to prevent:** `Foreman.reconciler` is a `Box<dyn Reconciler>` (`foreman.rs:145`). If the `Box<R>` forwarder does not forward `observe`, the entire feature silently no-ops in production *and every test still passes*. The forwarder tests below are not ceremony.

`PanicCatching::observe` wraps the call in `std::panic::catch_unwind(AssertUnwindSafe(…))` like its siblings and returns `Observations::default()` on a panic — it cannot return `Err`.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::observe::{Observation, Unknowable};
    use scroll_format::{ContentId, Glyph};

    struct Silent;
    impl Reconciler for Silent {
        fn apply(&self, glyph: &Glyph, cid: ContentId) -> EnactResult<Outcome> {
            Ok(Outcome { op: GlyphOp::Install { cid, glyph: glyph.clone() }, cid,
                         inverse: Inverse::Nothing, changed: false })
        }
        fn reverse(&self, _outcome: &Outcome) -> EnactResult<()> { Ok(()) }
    }

    struct Speaking;
    impl Reconciler for Speaking {
        fn apply(&self, glyph: &Glyph, cid: ContentId) -> EnactResult<Outcome> {
            Ok(Outcome { op: GlyphOp::Install { cid, glyph: glyph.clone() }, cid,
                         inverse: Inverse::Nothing, changed: false })
        }
        fn reverse(&self, _outcome: &Outcome) -> EnactResult<()> { Ok(()) }
        fn observe(&self, ops: &[GlyphOp]) -> Observations {
            ops.iter().map(|op| (op.key(), Observation::Realized)).collect()
        }
    }

    struct Panicking;
    impl Reconciler for Panicking {
        fn apply(&self, _glyph: &Glyph, _cid: ContentId) -> EnactResult<Outcome> {
            unreachable!()
        }
        fn reverse(&self, _outcome: &Outcome) -> EnactResult<()> { Ok(()) }
        fn observe(&self, _ops: &[GlyphOp]) -> Observations {
            panic!("the probe blew up")
        }
    }

    fn apt_op(name: &str) -> GlyphOp {
        let glyph = Glyph::AptPackage(scroll_format::AptPackage { name: name.to_string() });
        GlyphOp::Install { cid: crate::reconcile::glyph_content_id(&glyph), glyph }
    }

    #[test]
    fn a_reconciler_that_does_not_model_the_host_reports_not_modelled() {
        let ops = vec![apt_op("nginx")];
        assert_eq!(
            Silent.observe(&ops).get("apt:nginx"),
            Observation::Unknown(Unknowable::NotModelled)
        );
    }

    #[test]
    fn a_boxed_reconciler_forwards_observe_to_the_inner_one() {
        let boxed: Box<dyn Reconciler> = Box::new(Speaking);
        let ops = vec![apt_op("nginx")];
        assert_eq!(boxed.observe(&ops).get("apt:nginx"), Observation::Realized);
    }

    #[test]
    fn an_arced_reconciler_forwards_observe_to_the_inner_one() {
        let shared: std::sync::Arc<dyn Reconciler> = std::sync::Arc::new(Speaking);
        let ops = vec![apt_op("nginx")];
        assert_eq!(shared.observe(&ops).get("apt:nginx"), Observation::Realized);
    }

    #[test]
    fn panic_catching_forwards_observe_when_the_probe_behaves() {
        let guarded = PanicCatching::new(Speaking);
        let ops = vec![apt_op("nginx")];
        assert_eq!(guarded.observe(&ops).get("apt:nginx"), Observation::Realized);
    }

    #[test]
    fn a_panicking_probe_degrades_to_no_observations() {
        let guarded = PanicCatching::new(Panicking);
        let ops = vec![apt_op("nginx")];
        let observed = guarded.observe(&ops);
        assert!(observed.is_empty());
        assert_eq!(
            observed.get("apt:nginx"),
            Observation::Unknown(Unknowable::NotModelled)
        );
    }
}
```

Adjust the `Glyph::AptPackage` construction to whatever `scroll-format` actually spells (check `libs/scroll-format/src/scroll.rs`); the assertion is the point, not the constructor.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `direnv exec . cargo test -p golemd reconciler::`
Expected: FAIL — `no method named observe`.

- [ ] **Step 3: Write the minimal implementation**

The defaulted trait method plus three forwarders. Nothing else.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `direnv exec . cargo test -p golemd reconciler::` → PASS, 6 tests.
Run: `direnv exec . cargo test --workspace` → PASS. **This is the load-bearing run for this task**: it proves all 32 existing `impl Reconciler` still compile untouched.

- [ ] **Step 5: Commit** (via `lw:historian`)

---

### Task 3: `FakeReconciler::observe` and its out-of-band seams

**Files:**
- Modify: `apps/golemd/src/fake_reconciler.rs`
- Test: `apps/golemd/src/fake_reconciler.rs` (new `#[cfg(test)] mod tests` — this file currently has none)

**Interfaces:**
- Consumes: Task 1's `Observation`/`Unknowable`/`Observations`; Task 2's `Reconciler::observe`.
- Produces:
  ```rust
  impl FakeReconciler {
      pub fn preexisting(self, key: &str, cid: ContentId) -> Self;
      pub fn vanished(self, key: &str) -> Self;
  }
  ```
  Both are consuming builders, matching the existing `with_keyring(mut self, …) -> Self` shape at `fake_reconciler.rs:29`.

**Why the seams exist:** the fake's `present: Mutex<BTreeMap<String, ContentId>>` is golem's *own record* — the same information the journal holds. If `observe` read it naively the two columns could never disagree, and a fake that cannot produce disagreement gives the join/summary/render path (where this feature's entire risk lives) a harness that only exercises the happy path. `preexisting` writes the map with no apply having run; `vanished` removes a key whatever the fake applied. Implement `vanished` so a later `apply` can still re-add the key — it is a one-shot mutation of the map, not a sticky suppression.

**The verdict table** (from the design doc — do not improvise):

| op | map lookup | verdict |
|---|---|---|
| `Remove` | absent | `Absent` |
| `Remove` | present | `Realized` |
| other | absent | `Absent` |
| other | present, cid equal | `Realized` |
| other | present, cid differs | `Divergent` |
| any, `openable(glyph).is_err()` | — | `Unknown(Sealed)` (checked **first**) |

Honoring the keyring via the existing `openable` (`fake_reconciler.rs:43`) is not optional: it is the only way Task 5's keyless-host behavior gets tested against the default reconciler.

- [ ] **Step 1: Write the failing tests**

Seven tests, named for the behavior:

```rust
#[test] fn a_glyph_the_fake_applied_observes_as_realized() { … }
#[test] fn a_glyph_the_fake_never_applied_observes_as_absent() { … }
#[test] fn a_preexisting_glyph_at_a_different_cid_observes_as_divergent() { … }
#[test] fn a_preexisting_glyph_the_journal_never_saw_observes_as_realized() { … }
#[test] fn a_vanished_glyph_observes_as_absent_though_the_fake_applied_it() { … }
#[test] fn a_remove_of_a_glyph_still_on_the_fake_host_observes_as_realized() { … }
#[test] fn a_remove_of_a_glyph_already_gone_observes_as_absent() { … }
#[test] fn a_sealed_glyph_this_fake_cannot_open_observes_as_unknown_sealed() { … }
```

Representative body for the one that carries the design (the driving case: the host already has it, golem never applied it):

```rust
#[test]
fn a_preexisting_glyph_the_journal_never_saw_observes_as_realized() {
    let glyph = file_glyph("/etc/motd", "hello\n");
    let cid = crate::reconcile::glyph_content_id(&glyph);
    let fake = FakeReconciler::new().preexisting(&glyph.key(), cid);
    let ops = vec![GlyphOp::Install { cid, glyph: glyph.clone() }];

    assert_eq!(fake.observe(&ops).get(&glyph.key()), Observation::Realized);
}
```

And the one that proves the columns can disagree:

```rust
#[test]
fn a_preexisting_glyph_at_a_different_cid_observes_as_divergent() {
    let desired = file_glyph("/etc/motd", "hello\n");
    let on_host = file_glyph("/etc/motd", "ansible wrote this\n");
    let desired_cid = crate::reconcile::glyph_content_id(&desired);
    let host_cid = crate::reconcile::glyph_content_id(&on_host);
    let fake = FakeReconciler::new().preexisting(&desired.key(), host_cid);
    let ops = vec![GlyphOp::Install { cid: desired_cid, glyph: desired.clone() }];

    assert_eq!(fake.observe(&ops).get(&desired.key()), Observation::Divergent);
}
```

For the sealed test, build a `FakeReconciler::new()` with **no** keyring (the default `Keyring::without_key()`) and a glyph whose contents are a `Text::Composed` carrying a `Chunk::Hole(Secret::Sealed { … })`. Copy the sealed-`Text` construction from `apps/golemd/tests/secrets.rs`, which already builds these.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `direnv exec . cargo test -p golemd fake_reconciler::`
Expected: FAIL — `no method named preexisting`.

- [ ] **Step 3: Write the minimal implementation**

`preexisting`, `vanished`, and the `observe` impl from the design doc's code block.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `direnv exec . cargo test -p golemd fake_reconciler::` → PASS, 8 tests.
Run: `direnv exec . cargo test --workspace` → PASS.

- [ ] **Step 5: Commit** (via `lw:historian`)

---

### Task 4a: `HostReconciler::observe` — filesystem and `lineInFile`

**Files:**
- Modify: `apps/golemd/src/reconcilers.rs` — add `observe` to `impl<R: CommandRunner> Reconciler for HostReconciler<R>` (`:489`), plus free functions beside the existing `apply_*` helpers
- Test: `apps/golemd/src/reconcilers.rs` (the existing `#[cfg(test)] mod tests` at `:1109`)

**Interfaces:**
- Consumes: Task 2's `Reconciler::observe`; the existing `read_file` (`:833`), `observe_perms` (`:848`), `perms_match` (`:869`), `file_has_line` (`:1057`), `Keyring::open` (`secrets.rs:58`).
- Produces: this task lands `observe` handling `Glyph::Filesystem` (all three `Entry` arms) and `Glyph::LineInFile`; apt and systemd fall through to `Observation::Unknown(Unknowable::NotModelled)` until Task 4b. Say so in the test names so a reviewer sees the seam is deliberate.

**Semantics — reuse the exact predicates `apply` uses. Do not re-derive them.**

- `Entry::File`: unseal `contents` through `self.keyring.open(text, &glyph.key())`. On `Err` → `Unknown(Sealed)`. Then `read_file(path)`:
  - `Ok(None)` → `Absent`
  - `Ok(Some((prior, prior_perms)))` → `Realized` iff `prior == contents && perms_match(&prior_perms, perms)?`, else `Divergent`
  - `Err(_)` → `Unknown(Unreadable)` (this covers the non-UTF-8 `Fatal` `read_file` raises)
  - **Free early-out:** if `fs::metadata(path)`'s `len()` differs from `contents.len() as u64`, it is `Divergent` without reading the file. Equal contents implies equal length. Apply this before `read_file`.
- `Entry::Directory`: `fs::symlink_metadata(path)`:
  - `Err(NotFound)` → `Absent`; other `Err` → `Unknown(Unreadable)`
  - `Ok(meta)` where `!meta.is_dir()` → `Divergent`
  - `Ok(meta)` where `meta.is_dir()` → `Realized` iff `perms_match(&observe_perms(path)?, perms)?`, else `Divergent`
- `Entry::Symlink`: `fs::symlink_metadata(path)`:
  - `Err(NotFound)` → `Absent`; other `Err` → `Unknown(Unreadable)`
  - not a symlink → `Divergent`
  - symlink → `Realized` iff `fs::read_link(path)? == Path::new(target)`, else `Divergent`
- `LineInFile`: unseal `line`; `Err` → `Unknown(Sealed)`. Then `file_has_line(path, line)`: `true` → `Realized`, `false` → `Absent`, `Err` → `Unknown(Unreadable)`. **`lineInFile` never yields `Divergent`** — the line is present among others or it is not.
- **Read each `lineInFile` path once.** Build a `BTreeMap<String, Option<String>>` cache inside `observe` and consult it, so M line glyphs on `/etc/hosts` cost one read, not M. Missing file caches as `None` and reads `Absent`.
- **`Remove` ops ask presence only.** For a `Remove`, skip the content/perms comparison entirely and answer `Realized` if anything exists at the path (or the line is present) and `Absent` otherwise. This is what exempts removes from ever being `Unknown(Sealed)` — presence needs no key, so **do not unseal for a `Remove`**.
- **Dedupe by key.** Two ops with the same `Glyph::key()` probe once. `Observations` being a map makes the record idempotent, but skip the work too.

- [ ] **Step 1: Write the failing tests**

Follow the file's existing tempdir conventions (see `file_reapply_same_contents_and_mode_is_unchanged` at `:1976` for the shape). Twelve tests:

```rust
#[test] fn a_file_the_host_already_holds_byte_for_byte_observes_as_realized() { … }
#[test] fn a_file_with_different_contents_observes_as_divergent() { … }
#[test] fn a_file_of_a_different_length_observes_as_divergent_without_reading_it() { … }
#[test] fn a_file_with_matching_contents_and_a_different_mode_observes_as_divergent() { … }
#[test] fn a_file_that_is_not_there_observes_as_absent() { … }
#[test] fn a_directory_that_exists_with_the_asked_mode_observes_as_realized() { … }
#[test] fn a_directory_path_holding_a_plain_file_observes_as_divergent() { … }
#[test] fn a_symlink_pointing_where_asked_observes_as_realized() { … }
#[test] fn a_symlink_pointing_elsewhere_observes_as_divergent() { … }
#[test] fn a_line_already_in_the_file_observes_as_realized() { … }
#[test] fn a_line_missing_from_the_file_observes_as_absent() { … }
#[test] fn many_line_glyphs_on_one_path_read_that_path_once() { … }
#[test] fn a_remove_of_a_file_still_on_disk_observes_as_realized() { … }
#[test] fn a_remove_of_a_file_already_deleted_observes_as_absent() { … }
#[test] fn a_sealed_file_this_host_cannot_open_observes_as_unknown_sealed() { … }
#[test] fn a_remove_of_a_sealed_file_still_observes_by_presence_alone() { … }
#[test] fn observe_never_writes_anything_to_the_host() { … }
```

The two that carry the most weight:

```rust
#[test]
fn a_file_the_host_already_holds_byte_for_byte_observes_as_realized() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("motd");
    std::fs::write(&path, "ansible wrote this\n").unwrap();
    let glyph = file_glyph(path.to_str().unwrap(), "ansible wrote this\n");
    let rec = HostReconciler::with_runner(FakeCommandRunner::new());
    let ops = vec![install_op(&glyph)];

    assert_eq!(rec.observe(&ops).get(&glyph.key()), Observation::Realized);
}

#[test]
fn observe_never_writes_anything_to_the_host() {
    let dir = tempfile::tempdir().unwrap();
    let present = dir.path().join("present");
    std::fs::write(&present, "old\n").unwrap();
    let absent = dir.path().join("absent");

    let rec = HostReconciler::with_runner(FakeCommandRunner::new());
    let ops = vec![
        install_op(&file_glyph(present.to_str().unwrap(), "new\n")),
        install_op(&file_glyph(absent.to_str().unwrap(), "new\n")),
        install_op(&dir_glyph(dir.path().join("nested").to_str().unwrap())),
        install_op(&symlink_glyph(dir.path().join("link").to_str().unwrap(), "/tmp")),
        install_op(&line_glyph(absent.to_str().unwrap(), "a line")),
    ];

    rec.observe(&ops);

    assert_eq!(std::fs::read_to_string(&present).unwrap(), "old\n");
    assert!(!absent.exists());
    assert!(!dir.path().join("nested").exists());
    assert!(!dir.path().join("link").exists());
}
```

`many_line_glyphs_on_one_path_read_that_path_once` needs a read counter. If the file's helpers do not give one, assert the weaker but still useful property that N line glyphs on one path produce N correct verdicts, and leave the caching assertion to a comment-free structural test only if it can be made honestly — **do not fake a passing assertion**. Say plainly in the review package if you could not test the cache directly.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `direnv exec . cargo test -p golemd reconcilers::tests::` and grep for the new names.
Expected: FAIL — the default `observe` returns empty, so every assertion reads `Unknown(NotModelled)`.

- [ ] **Step 3: Write the minimal implementation**

- [ ] **Step 4: Run the tests to verify they pass**

Run: `direnv exec . cargo test -p golemd` → PASS, all pre-existing tests plus the new ones.

- [ ] **Step 5: Commit** (via `lw:historian`)

---

### Task 4b: `HostReconciler::observe` — apt (batched) and systemd

**Files:**
- Modify: `apps/golemd/src/reconcilers.rs`
- Test: `apps/golemd/src/reconcilers.rs` (existing test module), and `apps/golemd/src/host.rs` if `FakeCommandRunner` needs a way to script batch `dpkg-query` stdout (extend it the way `with_installed`/`with_service` already do — do not rewrite it)

**Interfaces:**
- Consumes: Task 4a's `observe`; the existing `systemd_enabled` (`:299`), `systemd_active` (`:306`), `CommandRunner::run` (`host.rs:38`).
- Produces: `observe` now answers all four glyph kinds. Also a free function, unit-testable without a runner:
  ```rust
  fn dpkg_installed_names(stdout: &str) -> std::collections::BTreeSet<String>
  ```

**apt — batched, and the exit-code trap:**

One invocation for the whole scroll:

```
dpkg-query -W -f=${Package} ${Status}\n <name1> <name2> …
```

**All of the following was verified on a real Debian trixie guest before this task was written** — none of it is assumption:

- **`dpkg-query` exits nonzero when *any* requested name is unknown, and still prints the known ones to stdout.** `dpkg-query -W -f='${Package} ${Status}\n' bash nosuchpkg coreutils` prints `bash` and `coreutils` on stdout and exits 1; the `no packages found matching nosuchpkg` line goes to **stderr**. The batch parser must therefore read stdout **regardless of exit status**, and must not be confused by stderr. `apt_installed`'s `query.succeeded()` gate (`:211`) is correct for one name and *wrong* for the batch — do not reuse it unchanged.
- **Use `${Package}`, not `${binary:Package}`.** `${binary:Package}` emits an architecture qualifier for every Multi-Arch: same package — `libc6:amd64`, `libacl1:amd64`, `gcc-14-base:amd64` and hundreds more on a stock trixie box. A glyph `aptPackage { name = "libc6" }` would then observe as `Absent` on a host where it is installed, and the plan would lie. `${Package}` emits the plain name with **zero** colons across the entire dpkg database (verified by counting colons over every record). This removes the need for any stripping logic.
- **A package apt knows but that was never installed is absent from the dpkg database entirely** — no stdout line at all. So "absent from stdout" is the correct `Absent` signal. A removed-but-configured package reads `deinstall ok config-files`, which the exact-match requirement below already excludes.

`dpkg_installed_names` parses each line as `<package> <status…>` and keeps the package iff the rest of the line is exactly `install ok installed`.

`aptPackage` is **presence-only**: `Realized` or `Absent`, never `Divergent`. The probe must never touch the apt index — ADR 0030's rule that a `Latest` glyph never participates in drift detection.

**systemd — per-unit, deliberately not batched:**

Keep the existing `systemd_enabled` + `systemd_active` pair, exactly as `apply_systemd` (`:245`) calls them, trading ~2N cheap spawns for exact predicate identity. `systemctl show --property=UnitFileState,ActiveState,LoadState` would collapse it to one spawn but `UnitFileState`'s value set is not exit-code-identical to `is-enabled`'s (`static`, `indirect`, `generated`, `linked` all exit 0), and getting that allowlist wrong makes the plan lie about a running service. Deferred; already recorded in `docs/TODO.md`.

- `Realized` iff `systemd_enabled(unit)? && systemd_active(unit)?`
- `Absent` when `is-enabled`'s **stdout is empty** — the not-found signal, free from output the probe already captures
- `Divergent` otherwise (unit known, but not both enabled and active)
- `Err` from either probe → `Unknown(Unreadable)`
- A `Remove` of a systemd glyph asks presence: `Realized` if the unit is known at all, `Absent` if not found.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn dpkg_batch_output_yields_only_the_fully_installed_names() { … }   // pure, on dpkg_installed_names
#[test] fn a_removed_but_configured_package_is_not_counted_as_installed() { … }  // "deinstall ok config-files"
#[test] fn a_half_configured_package_is_not_counted_as_installed() { … }
#[test] fn every_apt_glyph_in_a_scroll_is_probed_in_one_dpkg_query() { … }
#[test] fn a_nonzero_dpkg_exit_still_yields_the_names_that_were_found() { … }
#[test] fn an_installed_package_observes_as_realized() { … }
#[test] fn a_missing_package_observes_as_absent() { … }
#[test] fn an_apt_glyph_never_observes_as_divergent() { … }
#[test] fn a_unit_that_is_enabled_and_active_observes_as_realized() { … }
#[test] fn a_unit_that_is_enabled_but_inactive_observes_as_divergent() { … }
#[test] fn a_unit_systemd_does_not_know_observes_as_absent() { … }
#[test] fn a_remove_of_a_unit_still_installed_observes_as_realized() { … }
```

The two that pin the traps:

```rust
#[test]
fn a_nonzero_dpkg_exit_still_yields_the_names_that_were_found() {
    let names = dpkg_installed_names(
        "nginx install ok installed\ncurl install ok installed\n",
    );
    assert!(names.contains("nginx"));
    assert!(names.contains("curl"));
    assert!(!names.contains("jq"));
}

#[test]
fn every_apt_glyph_in_a_scroll_is_probed_in_one_dpkg_query() {
    let runner = FakeCommandRunner::new().with_installed(["nginx", "curl"]);
    let rec = HostReconciler::with_runner(runner);
    let ops = vec![install_op(&apt_glyph("nginx")),
                   install_op(&apt_glyph("curl")),
                   install_op(&apt_glyph("jq"))];

    let observed = rec.observe(&ops);

    assert_eq!(observed.get("apt:nginx"), Observation::Realized);
    assert_eq!(observed.get("apt:curl"), Observation::Realized);
    assert_eq!(observed.get("apt:jq"), Observation::Absent);
    let log = runner_of(&rec).log();
    assert_eq!(log.iter().filter(|l| l.contains("dpkg-query")).count(), 1);
}
```

The second test requires `FakeCommandRunner` to answer a multi-name `dpkg-query -W` with one line per installed name it knows. Extend `host.rs`'s `fake` module for that; keep the existing single-name behavior working so `apt_isometry_when_present_leaves_it` (`:1244`) and friends stay green.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `direnv exec . cargo test -p golemd reconcilers::tests::`
Expected: FAIL.

- [ ] **Step 3: Write the minimal implementation**

- [ ] **Step 4: Run the tests to verify they pass**

Run: `direnv exec . cargo test -p golemd` → PASS. Confirm the 44 pre-existing `reconcilers.rs` tests are all still green — especially the apt ones, which share the `FakeCommandRunner` you just extended.

- [ ] **Step 5: Commit** (via `lw:historian`)

---

### Task 5: `PlanScope`, the wire fields, and the join in the foreman

**Files:**
- Modify: `apps/golemd/src/plan_report.rs` — new `Observed`, `Unobservable`, `Reality`; new optional fields on `PlannedOp` and `PlanReport`
- Modify: `apps/golemd/src/foreman.rs` — `PlanScope`, `plan_manifest_scoped`, `plan_manifest` delegating
- Test: `apps/golemd/src/plan_report.rs` (existing tests at `:108-229`) and `apps/golemd/src/foreman.rs` (existing plan tests at `:5680-6180`)

**Interfaces:**
- Consumes: Tasks 1–4's `Observation`/`Observations`/`Reconciler::observe`.
- Produces:
  ```rust
  // foreman.rs
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum PlanScope { JournalOnly, JournalAndHost }

  impl Foreman {
      pub fn plan_manifest(&self, bytes: &[u8]) -> Result<PlanReport, ForemanError> {
          self.plan_manifest_scoped(bytes, PlanScope::JournalOnly)
      }
      pub fn plan_manifest_scoped(&self, bytes: &[u8], scope: PlanScope)
          -> Result<PlanReport, ForemanError>;
  }

  // plan_report.rs — additions only, no reordering of existing fields
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
  #[serde(rename_all = "snake_case")]
  pub enum Observed { Realized, Divergent, Absent, Unknown }

  #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
  #[serde(rename_all = "snake_case")]
  pub enum Unobservable { Sealed, Unreadable, NotModelled }

  #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
  pub struct Reality {
      pub realized: usize,
      pub divergent: usize,
      pub absent: usize,
      pub unknown: usize,
      pub already_gone: usize,
      pub still_present: usize,
      pub host_already_matches: bool,
  }

  // on PlannedOp, appended after `describe`
  #[serde(skip_serializing_if = "Option::is_none")] pub observed: Option<Observed>,
  #[serde(skip_serializing_if = "Option::is_none")] pub unobservable: Option<Unobservable>,

  // on PlanReport, appended after `summary`
  #[serde(skip_serializing_if = "Option::is_none")] pub reality: Option<Reality>,
  ```

**Behavior:**

- `plan_manifest_scoped(bytes, JournalOnly)` does exactly what `plan_manifest` does today. It **must not call `observe`**. `observed`, `unobservable` and `reality` are all `None`, so the serialized body is byte-identical to today's.
- `plan_manifest_scoped(bytes, JournalAndHost)` builds the same op list, then calls `self.reconciler.observe(&all_ops)` **once**, with every op (unit ops and vanished removes together), and stamps each `PlannedOp` from `observations.get(&op.glyph_key)`.
- `Observation::Unknown(u)` maps to `observed: Some(Unknown)` **plus** `unobservable: Some(u.into())`. Every other observation leaves `unobservable: None`. `unobservable` is never `Some` without `observed == Some(Unknown)`.
- `Reality` counts over **distinct glyph keys**, not ops — a key declared in three units counts once. Removes are counted only in `already_gone` / `still_present`, never in `realized`/`divergent`/`absent`; non-removes only in the latter. `unknown` counts across both.
- `host_already_matches` is `true` iff `divergent == 0 && absent == 0 && unknown == 0 && still_present == 0` **and** at least one glyph was observed. An unknown never counts as agreement. An empty scroll is not a match.
- `reload` prediction is unchanged — it is derived from the journal diff, not from observations. Do not let observations change what reloads are predicted.

- [ ] **Step 1: Write the failing tests**

In `plan_report.rs`, serialization tests:

```rust
#[test] fn a_journal_only_plan_serializes_without_any_reality_fields() { … }
#[test] fn an_observed_op_carries_its_verdict_and_no_reason() { … }
#[test] fn an_unknown_op_carries_both_the_verdict_and_the_reason() { … }
#[test] fn reality_serializes_snake_case_counts() { … }
```

The first one is the byte-identity guard and should assert on the JSON directly:

```rust
#[test]
fn a_journal_only_plan_serializes_without_any_reality_fields() {
    let report = /* a PlanReport built with scope JournalOnly */;
    let json = serde_json::to_string(&report).unwrap();
    assert!(!json.contains("observed"));
    assert!(!json.contains("unobservable"));
    assert!(!json.contains("reality"));
}
```

In `foreman.rs`, beside the existing plan tests:

```rust
#[test] fn a_journal_only_plan_never_calls_observe() { … }   // counting reconciler
#[test] fn a_host_plan_stamps_every_op_with_its_observation() { … }
#[test] fn a_host_plan_calls_observe_exactly_once_for_the_whole_scroll() { … }
#[test] fn a_glyph_declared_in_three_units_counts_once_in_the_reality_summary() { … }
#[test] fn a_remove_whose_resource_is_gone_counts_as_already_gone() { … }
#[test] fn a_remove_whose_resource_remains_counts_as_still_present() { … }
#[test] fn a_host_that_already_holds_every_glyph_reports_host_already_matches() { … }
#[test] fn one_unknown_glyph_denies_host_already_matches() { … }
#[test] fn one_still_present_remove_denies_host_already_matches() { … }
#[test] fn an_empty_scroll_does_not_report_host_already_matches() { … }
#[test] fn a_host_plan_writes_nothing_to_the_journal() { … }
#[test] fn a_host_plan_predicts_the_same_reloads_as_a_journal_only_plan() { … }
```

The two that carry the driving case and the trap:

```rust
#[test]
fn a_host_that_already_holds_every_glyph_reports_host_already_matches() {
    // No prior revision at all — the enrollment case. The journal says
    // "install everything"; the host says it is already there.
    let glyphs = /* three glyphs */;
    let fake = glyphs.iter().fold(FakeReconciler::new(), |f, g| {
        f.preexisting(&g.key(), glyph_content_id(g))
    });
    let foreman = foreman_with(fake, MemoryPlanRoom::new());

    let report = foreman
        .plan_manifest_scoped(&manifest_bytes(&glyphs), PlanScope::JournalAndHost)
        .unwrap();

    assert_eq!(report.against_revision, None);
    assert_eq!(report.summary.install, 3);
    let reality = report.reality.unwrap();
    assert_eq!(reality.realized, 3);
    assert_eq!(reality.divergent, 0);
    assert!(reality.host_already_matches);
}

#[test]
fn one_unknown_glyph_denies_host_already_matches() {
    // Two glyphs the host holds, one sealed glyph this host cannot open.
    // An unknown is never agreement: calling this host a no-op would be
    // calling it safe on something golem could not check.
    let report = /* … JournalAndHost … */;
    let reality = report.reality.unwrap();
    assert_eq!(reality.realized, 2);
    assert_eq!(reality.unknown, 1);
    assert!(!reality.host_already_matches);
}
```

`a_journal_only_plan_never_calls_observe` needs a reconciler that increments an `AtomicUsize` in `observe`; write it as a small `#[cfg(test)]` double beside the existing ones (`Recorder` at `foreman.rs:2590` is the pattern).

- [ ] **Step 2: Run the tests to verify they fail**

Run: `direnv exec . cargo test -p golemd plan_report:: foreman::tests::`
Expected: FAIL.

- [ ] **Step 3: Write the minimal implementation**

- [ ] **Step 4: Run the tests to verify they pass**

Run: `direnv exec . cargo test -p golemd` → PASS. Especially confirm `a_plan_writes_nothing_to_the_journal` (`foreman.rs:5943`) and the four reload-prediction tests (`:5774-5926`) are untouched and green.

- [ ] **Step 5: Commit** (via `lw:historian`)

---

### Task 6: `POST /plan?against_host=true`

**Files:**
- Modify: `apps/golemd/src/http.rs` — the `/plan` route (`:56`) and `plan_manifest` handler (`:169`)
- Test: `apps/golemd/src/http.rs` (new `#[cfg(test)] mod tests`) or `apps/golemd/tests/` — follow whichever the crate already does for route tests; `apps/golemctl/src/conn.rs:506` shows an in-process axum router being driven, and `apps/golemd/tests/auth_gate.rs` is the integration precedent

**Interfaces:**
- Consumes: Task 5's `PlanScope` and `plan_manifest_scoped`.
- Produces:
  ```rust
  #[derive(Debug, Default, Deserialize)]
  struct PlanQuery {
      #[serde(default)]
      against_host: bool,
  }
  ```
  Handler signature gains `Query(query): Query<PlanQuery>`, mirroring the `?after=` pattern already used on `/reconciles/:id` (`http.rs:190`).

**Behavior:** absent or `false` → `PlanScope::JournalOnly`; `true` → `PlanScope::JournalAndHost`. An unparseable value is a 400 from axum's extractor — acceptable, do not hand-roll leniency.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn a_plan_with_no_query_string_is_journal_only() { … }
#[test] fn a_plan_with_against_host_true_reads_the_host() { … }
#[test] fn a_plan_with_against_host_false_is_journal_only() { … }
#[test] fn the_auth_gate_still_covers_the_host_plan() { … }
```

The first asserts the response JSON contains no `reality` key; the second that it does. The fourth extends the existing `auth_gate.rs` coverage — a probing plan must not be reachable without the bearer token.

- [ ] **Step 2: Run the tests to verify they fail** — `direnv exec . cargo test -p golemd http`
- [ ] **Step 3: Write the minimal implementation**
- [ ] **Step 4: Run the tests to verify they pass** — `direnv exec . cargo test -p golemd`
- [ ] **Step 5: Commit** (via `lw:historian`)

---

### Task 7a: golemctl — the flag, the transport, the wire mirror

**Files:**
- Modify: `apps/golemctl/src/main.rs` — `Cmd::Plan` gains `--against-host`, threaded to `plan::run`
- Modify: `apps/golemctl/src/conn.rs` — `post_plan` gains the query parameter
- Modify: `apps/golemctl/src/plan.rs` — the Deserialize mirrors and `run`'s signature
- Test: `apps/golemctl/src/plan.rs`, `apps/golemctl/src/conn.rs`

**Interfaces:**
- Consumes: Task 6's `?against_host=`.
- Produces:
  ```rust
  // main.rs
  Plan {
      source: PathBuf,
      addr: String,
      #[arg(long)] json: bool,
      #[arg(long)] detail: bool,
      #[arg(long)] against_host: bool,     // clap renders this as --against-host
  }

  // conn.rs
  pub async fn post_plan(&self, bytes: Vec<u8>, against_host: bool) -> Result<String>;

  // plan.rs
  pub async fn run(bytes: Vec<u8>, conn: &Conn, json: bool, detail: bool,
                   against_host: bool) -> Result<()>;

  #[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
  #[serde(rename_all = "snake_case")]
  pub enum Observed { Realized, Divergent, Absent, Unknown, #[serde(other)] Unrecognized }

  #[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
  #[serde(rename_all = "snake_case")]
  pub enum Unobservable { Sealed, Unreadable, NotModelled, #[serde(other)] Unrecognized }

  #[derive(Debug, Clone, Copy, Deserialize)]
  pub struct Reality { /* the seven fields from Task 5, all #[serde(default)] */ }

  // on PlannedOp
  #[serde(default)] pub observed: Option<Observed>,
  #[serde(default)] pub unobservable: Option<Unobservable>,
  // on PlanResponse
  #[serde(default)] pub reality: Option<Reality>,
  ```

The `#[serde(other)] Unrecognized` catch-alls are load-bearing: a future golemd variant must degrade one row, not fail the whole parse.

`RenderOptions` gains nothing in this task. Rendering is Task 7b; here the flag only changes the request and the parse.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn a_response_without_reality_fields_still_parses() { … }
#[test] fn an_op_carrying_an_observation_parses_it() { … }
#[test] fn an_unrecognized_observation_degrades_to_unrecognized_not_an_error() { … }
#[test] fn a_reality_block_parses_all_seven_counters() { … }
#[test] fn json_mode_passes_the_reality_fields_through_verbatim() { … }
```

Plus, in `conn.rs`, a round-trip against the in-process router (`conn.rs:506` is the existing harness):

```rust
#[tokio::test] async fn post_plan_without_against_host_sends_no_query_string() { … }
#[tokio::test] async fn post_plan_with_against_host_sends_the_query_parameter() { … }
```

The backward-compatibility test is the important one:

```rust
#[test]
fn a_response_without_reality_fields_still_parses() {
    // An older golemd, or a journal-only plan. Every new field is optional.
    let body = r#"{"host":"web-01","scroll_content_id":"3f9c1a",
                   "against_revision":12,"ops":[],"reloads":[],
                   "summary":{"install":0,"replace":0,"remove":0,"noop":0}}"#;
    let parsed: PlanResponse = serde_json::from_str(body).unwrap();
    assert!(parsed.reality.is_none());
}
```

- [ ] **Step 2: Run the tests to verify they fail** — `direnv exec . cargo test -p golemctl plan:: conn::`
- [ ] **Step 3: Write the minimal implementation**
- [ ] **Step 4: Run the tests to verify they pass**

Run: `direnv exec . cargo test -p golemctl` → PASS. **The 17 golden render tests at `plan.rs:798-1360` must be green and unmodified.** If one needed editing, stop and report — that means the default path changed.

- [ ] **Step 5: Commit** (via `lw:historian`)

---

### Task 7b: golemctl — the host block

**Files:**
- Modify: `apps/golemctl/src/plan.rs` — `RenderOptions`, `render`, a new `host_block`, `headline`, `footer`
- Test: `apps/golemctl/src/plan.rs` (the golden-render test module)

**Interfaces:**
- Consumes: Task 7a's `Observed`/`Unobservable`/`Reality` on the parsed response.
- Produces: `RenderOptions` gains `pub against_host: bool`. `render` emits the host block only when the response carries a `reality`.

**The rendering contract.** With `--against-host` off, output is byte-identical to today apart from the enrollment hint below: no `journal` label, no host block, no extra footer segments. With it on:

- The headline gains ` · against the host` after the `against …` clause.
- The journal steps get a `  journal` label line above them; the host block gets a `  host` label line.
- Reality marks are `=` / `≠` / `?`, disjoint from `+ ~ - ↻`. The word after the mark does the remove-inversion:

  | journal action | observed | renders |
  |---|---|---|
  | install/replace/noop | realized | `= match` |
  | install/replace/noop | absent | `≠ missing` |
  | install/replace/noop | divergent | `≠ differs` |
  | remove | absent | `= gone` |
  | remove | realized/divergent | `≠ present` |
  | any | unknown | `? unknown` |

- The host block lists every `≠` and `?` row in full, grouped by kind exactly as the journal block groups, and **collapses all `=` into one row** (`= match   N glyphs`). `--detail` expands the `=` row per kind with members.
- **A journal `noop` that is host-`divergent` still appears in the host block.** The two blocks are deliberately not row-for-row parallel — the journal block keeps its footer-only Noop treatment, the host block covers every desired glyph. That row is the single most important drift signal there is.
- The footer gains a `host · …` line summarizing the counters, and when `host_already_matches` a plain-language line: `host · every declared glyph already matches — applying this manifest changes nothing`.
- Each `? unknown` row gets a reason line naming the glyph and the `Unobservable` cause, e.g. `/etc/app/creds.conf is sealed and this host has no fleet key`. Never a bare `?`.
- **Enrollment hint, the one change to default output.** Append one dim footer line when `--against-host` was **not** passed and golem has committed nothing on this host:
  `  no prior revision here · --against-host checks what this host already has`

  **The condition is `against_revision.is_none() || against_revision == Some(1)`, not `is_none()` alone.** This was corrected mid-implementation after Task 5 found that `wal::latest_revision_id` (`apps/golemd/src/wal.rs:126`) returns `Some(1 + committed_attempt_count)` and **never returns `None`** — so a naive `is_none()` check would never fire on a real host, and the feature's own discoverability affordance would be dead code. Revision `1` is the `Init` revision: it means zero committed attempts, i.e. golem has applied nothing here. That is exactly the enrollment case. `None` stays in the condition because golemctl's headline already handles it and render tests construct it.

  **Suppress the hint in `nested` (fleet) mode.** Per-host blocks in a fan-out must stay compact, and the hint would repeat once per host.

  This is the **one sanctioned change to default output**. It changes the golden at `plan.rs:1070-1078`, which currently reads:

```
Plan for web-01 · against no prior revision · manifest 3f9c1a…

  no changes · 1 unchanged
```

  Update that golden to carry the hint line. **Any golden whose `against_revision` is `None` or `Some(1)` and which is not `nested` will also gain the line — that is correct, not drift.** Update those too, and list every golden you changed in your report so the reviewer can check the count against the condition. What would be drift is a golden at revision 2 or higher changing, or a `nested` one changing; if that happens, stop and report.

  **`apps/golemctl/tests/fleet_fanout.rs:220` asserts the nested headline at "against revision 1"** — this is precisely why the hint is suppressed in nested mode. That assertion must stay green and unedited; if it fails, the nested suppression is missing.

  Two live documentation fences may render the hint: `sites/website/src/content/docs/tutorials/a-failing-unit.mdx:74-91` and `tutorials/website-loop.mdx:106-110`. **Both are candidates now** — an earlier draft of this plan wrongly said the second was safe because it shows "against revision 1"; under the corrected condition that is exactly a hint case. Check both against real output. Task 10 owns fixing any fence that changes.

- [ ] **Step 1: Write the failing golden tests**

New goldens, asserted as whole strings the way the existing 17 are:

```rust
#[test] fn without_the_flag_the_render_is_unchanged() { … }
#[test] fn the_enrollment_case_shows_every_glyph_already_matching() { … }
#[test] fn the_drift_case_shows_the_two_columns_disagreeing() { … }
#[test] fn a_journal_noop_that_the_host_contradicts_appears_in_the_host_block() { … }
#[test] fn a_remove_whose_resource_is_gone_renders_as_gone_not_missing() { … }
#[test] fn a_remove_whose_resource_remains_renders_as_present() { … }
#[test] fn an_unknown_row_names_the_glyph_and_the_reason() { … }
#[test] fn every_matching_glyph_collapses_to_one_row() { … }
#[test] fn detail_expands_the_matching_glyphs_per_kind() { … }
#[test] fn a_plan_with_no_prior_revision_and_no_flag_hints_at_against_host() { … }
#[test] fn a_plan_against_a_revision_does_not_hint() { … }
#[test] fn the_nested_fleet_form_indents_both_blocks() { … }
#[test] fn no_color_mode_emits_no_sgr_codes_in_the_host_block() { … }
```

The enrollment golden, verbatim (this is the feature's whole point — the exact expected string):

```
Plan for web-01 · against no prior revision · against the host · manifest 3f9c1a…

  journal
  + install 12 apt packages  nginx curl jq git vim htop tmux rsync …
                             (web/base, web/extra)
  + install  4 files         /etc/nginx/nginx.conf /etc/motd … (web/nginx, web/base)
  + install  1 systemd unit  nginx.service (web/nginx)

  host
  = match   17 glyphs

  17 changes · 17 install
  host · every declared glyph already matches — applying this manifest changes nothing
```

and the drift golden:

```
Plan for web-01 · against revision 12 · against the host · manifest 3f9c1a…

  journal
  + install 3 apt packages  nginx curl jq (web/base, web/extra)
  ~ replace 2 files         /etc/systemd/system/nginx.service /etc/motd (web/nginx, web/base)
  - remove  1 line-in-file  /etc/hosts: "10.0.0.3 oldhost" (web/<removes>)
  ↻ restart 1 unit          nginx.service ← /etc/systemd/system/nginx.service

  host
  ≠ missing 1 apt package   nginx
  ≠ differs 1 file          /etc/motd
  ≠ present 1 line-in-file  /etc/hosts: "10.0.0.3 oldhost"
  ? unknown 1 file          /etc/app/creds.conf
  = match   4 glyphs

  6 changes · 3 install, 2 replace, 1 remove · 1 unchanged
  host · 3 disagree, 1 unreadable, 4 match
  /etc/app/creds.conf is sealed and this host has no fleet key
```

Exact column widths are yours to settle against `MARGIN`/`VERB_WIDTH`/`KIND_GAP` (`plan.rs:87-103`) — keep the host block's member column aligned with the journal block's so an operator can scan straight down. If the golden above disagrees with what the existing width constants produce, **the constants win and you update the golden**; report the difference.

- [ ] **Step 2: Run the tests to verify they fail** — `direnv exec . cargo test -p golemctl plan::`
- [ ] **Step 3: Write the minimal implementation**
- [ ] **Step 4: Run the tests to verify they pass**

Run: `direnv exec . cargo test -p golemctl` → PASS, the 17 original goldens unmodified.

- [ ] **Step 5: Commit** (via `lw:historian`)

---

### Task 8: `golemctl fleet plan --against-host`

**Files:**
- Modify: `apps/golemctl/src/main.rs` — `FleetCmd::Plan` gains the flag
- Modify: `apps/golemctl/src/fleet.rs` — `run_plan`, `gather_plans`, `plan_lines`, `plan_json` thread it through
- Test: `apps/golemctl/src/fleet.rs`, `apps/golemctl/tests/fleet_fanout.rs`

**Interfaces:**
- Consumes: Task 7a's `post_plan(bytes, against_host)` and Task 7b's `RenderOptions { nested: true, against_host }`.
- Produces: no new public types — a boolean threaded to the existing fan-out.

**Behavior:** every host in the inventory is probed, concurrently as today. A host whose probe reports unknowns is not an error and does not change the exit code; `fleet plan`'s existing exit-1-on-host-error rule (`fleet.rs:729-734`) is unchanged. The nested render indents both blocks under the per-host heading and drops the blank lines, as `nested` already does.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn fleet_plan_forwards_against_host_to_every_endpoint() { … }
#[test] fn fleet_plan_without_the_flag_is_unchanged() { … }
#[test] fn one_host_full_of_unknowns_does_not_fail_the_fleet_plan() { … }
#[tokio::test] async fn a_fleet_host_plan_reports_every_host_and_journals_nothing() { … }
```

The last mirrors the existing `a_fleet_plan_reports_every_host_and_journals_nothing` (`tests/fleet_fanout.rs:220`) with the flag on — write it as a sibling, do not edit the original.

- [ ] **Step 2: Run the tests to verify they fail** — `direnv exec . cargo test -p golemctl fleet`
- [ ] **Step 3: Write the minimal implementation**
- [ ] **Step 4: Run the tests to verify they pass** — `direnv exec . cargo test -p golemctl`
- [ ] **Step 5: Commit** (via `lw:historian`)

---

### Task 9: `apps/fleet` passthrough

**Files:**
- Modify: `apps/fleet/cli.py` — the `plan` command (`:273`), the argv it builds (`:296-309`)
- Test: `apps/fleet/tests/test_apply_render.py` (beside the existing `plan` argv test)

**Interfaces:**
- Consumes: Task 8's `golemctl fleet plan --against-host`.
- Produces: `fleet plan <source> [--hosts a,b] [--json] [--detail] [--against-host]`.

**Behavior:** the flag is appended to the `golemctl fleet plan` argv when set, and absent otherwise. Nothing else in the harness changes — `apps/fleet` parses no plan JSON and mirrors no plan types.

- [ ] **Step 1: Write the failing tests**

```python
def test_plan_forwards_against_host_to_golemctl(self):
    ...
    self.assertIn("--against-host", argv)

def test_plan_without_against_host_does_not_pass_the_flag(self):
    ...
    self.assertNotIn("--against-host", argv)
```

Copy the mocking shape from the existing `test_plan_execs_golemctl_fleet_plan_once_naming_every_host` (`test_apply_render.py:167`).

- [ ] **Step 2: Run the tests to verify they fail**

Run: `direnv exec . env PYTHONPATH=/home/lakin/personal-repos/dull-ca/golem/apps python -m unittest discover -s /home/lakin/personal-repos/dull-ca/golem/apps/fleet/tests`
Expected: FAIL.

- [ ] **Step 3: Write the minimal implementation**
- [ ] **Step 4: Run the tests to verify they pass** — same command, all ~70 tests green.
- [ ] **Step 5: Commit** (via `lw:historian`)

---

### Task 10: The published docs

**Files:**
- Modify: `sites/website/src/content/docs/reference/cli.mdx` — the `golemctl plan` entry (`:120-126`), the `fleet plan` entry (`:128-135`), the `/plan` row of the HTTP endpoint table (`:299`)
- Modify: `sites/website/src/content/docs/getting-started/applying.mdx` (`:22-32`) — the two-column concept and the enrollment story
- Modify: `sites/website/src/content/docs/reference/status.mdx` (`:58-59`)
- Modify: `sites/website/src/content/docs/explanation/trust.mdx` (near `:77`) — the *verdict, never plaintext* invariant
- Modify: `sites/website/src/content/docs/explanation/architecture.mdx` — the diff section (`:88-95`) currently says the diff is journal-only with no side effects; it needs the second column named without contradicting the purity claim
- Check, and fix only if the enrollment hint line changes their output: `sites/website/src/content/docs/tutorials/a-failing-unit.mdx:74-91`, `tutorials/website-loop.mdx:106-110`
- Test: the ADR 0054 documentation gate — `direnv exec . nix flake check`

**Interfaces:**
- Consumes: the shipped CLI surface from Tasks 7a/7b/8, and ADR 0058 for the reasoning. Read ADR 0058 before writing — `docs/adr/` is internal and must **not** be linked from the public site (per `docs/TODO.md:443`, a "see ADR NNNN" on a public page is a dangling pointer).

**What to say, and what not to:**
- `--against-host` is opt-in for **cost and surprise, not security**. Say that plainly. Someone will otherwise read it as a permission boundary.
- The enrollment story is the headline: you can prove a host golem has never touched already matches the manifest, before touching it.
- The invariant on `trust.mdx`: a verdict crosses the wire, never host state. Even for a secret-bearing file, all `plan` learns is `match` / `differs` / `missing` / `unknown`.
- `architecture.mdx` must not be edited into saying the diff has side effects. The diff is still pure; the probe runs beside it.

- [ ] **Step 1: Read ADR 0058 and the six pages listed above.**
- [ ] **Step 2: Run the gate before touching anything to confirm it is green** — `direnv exec . nix flake check --print-build-logs`
- [ ] **Step 3: Write the prose and update the fences.**
- [ ] **Step 4: Run the gate again** — `direnv exec . nix flake check --print-build-logs`. Expected: PASS. Every fence and every internal link is checked (ADR 0054).
- [ ] **Step 5: Commit** (via `lw:historian`)

---

### Task 11: A `--reconciler fake` end-to-end smoke through the real binaries

**Files:**
- Create: `apps/golemctl/tests/plan_against_host.rs`
- Test: itself

**Interfaces:**
- Consumes: everything above.
- Produces: nothing other tasks depend on. This is the seam between the unit tests and the VM run — it proves the whole chain (clap flag → query param → foreman → `observe` → wire → render) holds together in one process before Task 12 spends minutes on a VM.

**Behavior:** spin a real golemd router in-process with a `FakeReconciler` seeded via `preexisting`, drive it through `Conn`, and assert the rendered two-column output. `apps/golemctl/tests/fleet_fanout.rs:58` and `apps/golemctl/src/conn.rs:506` both show how to stand one up.

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test] async fn a_host_already_holding_every_glyph_plans_as_a_provable_no_op() { … }
#[tokio::test] async fn a_glyph_changed_behind_golems_back_makes_the_two_columns_disagree() { … }
#[tokio::test] async fn a_host_plan_writes_no_revision() { … }
```

The second must assert *both* columns: journal says `noop` (golem applied it, cid unchanged) and host says `differs` (the fake was told `preexisting` at another cid). That disagreement is the feature.

- [ ] **Step 2: Run the tests to verify they fail** — `direnv exec . cargo test -p golemctl --test plan_against_host`
- [ ] **Step 3: Write the minimal implementation** (test-only task — if it fails, the bug is in an earlier task; fix it there and note it in the review package)
- [ ] **Step 4: Run the tests to verify they pass** — `direnv exec . cargo test --workspace`
- [ ] **Step 5: Commit** (via `lw:historian`)

---

### Task 12: The VM proof — two diffs disagreeing on a real Debian guest

**Files:** none created. This task produces **observed terminal output**, which is the deliverable.

**Interfaces:**
- Consumes: the whole feature, built.
- Produces: the transcript that goes into the final report. Nothing may claim this feature works until this task's output exists.

**Prerequisites, already verified present on this machine:** qemu 10.2.2, `/dev/kvm` (mode 0666, `KVM_GET_API_VERSION` = 12), `cloud-localds`, nix 2.34.8, the 340 MB Debian trixie base image already cached at `.fleet/images/`, `.fleet/golem-token` and the ssh keypair present. `.fleet/state.json` is absent (no VMs exist) and `.fleet/inventory.toml` is stale (it names a wrong repo path) — regenerate it, do not trust it.

- [ ] **Step 1: Build the binaries the harness needs**

```
direnv exec . cargo build --workspace
direnv exec . cargo build --release -p golemctl
```

**Do not run `cargo clean`. Do not delete anything under `target/`.** Another agent depends on `target/release/emetc`.

- [ ] **Step 2: Boot one guest and deploy golemd**

```
direnv exec . fleet up --hosts scaly
direnv exec . fleet deploy --hosts scaly
direnv exec . fleet status
```

Expect minutes: `fleet up` waits out cloud-init (180 s timeout, 3 s poll), `fleet deploy` runs `nix build .#golemd-static`. The guest runs `golemd --reconciler host`, which is the whole point.

- [ ] **Step 3: Apply `apps/fleet/smoke.emet` and confirm it settled**

```
direnv exec . fleet apply apps/fleet/smoke.emet --hosts scaly
direnv exec . fleet plan apps/fleet/smoke.emet --hosts scaly
```

The second must show **no changes** — the journal is now current. Record this output.

- [ ] **Step 4: Change the host behind golem's back**

`smoke.emet` writes `/etc/golem-smoke.conf`. Edit it in the guest by hand, exactly as Ansible would:

```
direnv exec . fleet ssh scaly -- sudo sh -c 'echo "# ansible was here" >> /etc/golem-smoke.conf'
direnv exec . fleet ssh scaly -- cat /etc/golem-smoke.conf
```

- [ ] **Step 5: Show the two diffs disagreeing — THE DELIVERABLE**

```
direnv exec . fleet plan apps/fleet/smoke.emet --hosts scaly
direnv exec . fleet plan apps/fleet/smoke.emet --hosts scaly --against-host
```

Expected, and this is the assertion: the first says **no changes** (the journal still holds the cid golem last applied, and the manifest has not moved). The second says the same in the journal column and `≠ differs` for `/etc/golem-smoke.conf` in the host column. **Two diffs, disagreeing, on a real host.** Capture both transcripts verbatim.

- [ ] **Step 6: Show the enrollment case — the driving scenario**

This is the case Dr. Dub actually needs. On a **fresh** guest with no golem journal, put the manifest's files in place by hand first, then plan:

```
direnv exec . fleet up --hosts manta
direnv exec . fleet deploy --hosts manta
# put smoke.emet's declared state on the box by hand, as Ansible would have:
direnv exec . fleet ssh manta -- sudo apt-get install -y htop
direnv exec . fleet ssh manta -- sudo sh -c 'cat > /etc/golem-smoke.conf' < <the exact contents smoke.emet declares>
# …and the lineInFile line
direnv exec . fleet plan apps/fleet/smoke.emet --hosts manta --against-host
```

Read the exact declared contents and mode out of `apps/fleet/smoke.emet` first — the file must match **byte for byte and mode for mode** or the point is not made. `direnv exec . cargo run -q -p emet -- build --text apps/fleet/smoke.emet` prints the readable plan if the source is ambiguous.

Expected: journal column says `+ install` for every glyph (no prior revision), host column says `= match` for every glyph, and the footer says `host · every declared glyph already matches — applying this manifest changes nothing`. **That is the proof that enrolling an Ansible-managed host is a no-op.** Capture it verbatim.

- [ ] **Step 7: Verify against reality that the claim is true**

Apply it and confirm nothing changed, closing the loop:

```
direnv exec . fleet apply apps/fleet/smoke.emet --hosts manta
```

Every glyph should report unchanged. If any glyph reports `changed`, **the reality diff lied** — that is a defect, report it, do not smooth it over.

- [ ] **Step 8: Note what the guest taught you about the two unverified assumptions**

Two behaviors were designed on unverified assumptions. Check them here:
- `dpkg-query -W -f='${binary:Package} ${Status}\n' n1 n2 …` with an unknown name — does it exit nonzero and still print the known ones? Run it in the guest and report the actual output and exit code.
- Does `${binary:Package}` emit an arch qualifier (`htop:amd64`) on this Debian?

Report both. If either differs from Task 4b's implementation, that is a defect to fix before completion.

- [ ] **Step 9: Tear down**

```
direnv exec . fleet ssh scaly -- sync
direnv exec . fleet ssh manta -- sync
direnv exec . fleet reset
```

- [ ] **Step 10: No commit** — this task produces evidence, not code. If it produced a fix, that fix commits under the task it belongs to.

---

## Self-review

**Spec coverage.** Q1 (remove inversion) → Tasks 4a/4b semantics, Task 5 counters, Task 7b render table. Q2 (purity) → Task 1's import restriction, Task 2's port, Task 5's `PlanScope`; `reconcile.rs` is touched by no task, which is the point. Q3 (privilege) → Task 3's `openable` gate, Task 4a's `Unknown(Sealed)`, Task 5's `host_already_matches` unknown rule, Task 6's auth-gate test, Task 10's `trust.mdx`. Q4 (cost) → Task 4a's read cache and length early-out, Task 4b's dpkg batch; the systemd deferral is recorded in `docs/TODO.md` by the ADR task. Q5 (surface) → Tasks 6, 7a, 7b, 8, 9, 10. Q6 (fake) → Tasks 2 and 3. The VM proof → Task 12. The ADR → written in parallel, outside this plan's task list.

**Type consistency.** `Observation`/`Unknowable`/`Observations` (golemd internal, Task 1) are distinct from `Observed`/`Unobservable`/`Reality` (wire, Task 5) and from golemctl's Deserialize twins (Task 7a). That is three vocabularies for one concept and it is deliberate — the internal enum carries a payload, the wire enum is flat, and the client's has a catch-all. An implementer must not collapse them. `PlanScope` is golemd-internal and never crosses the wire; the wire says `?against_host=`.

**Known gap.** Task 4a's `many_line_glyphs_on_one_path_read_that_path_once` may not be honestly assertable without a read counter the codebase does not have. The plan says so and forbids faking it.
