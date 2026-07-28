# Recursive Scroll Model Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `Scroll` a recursive, strict tree (`Scroll { name, policy, contents }` where `contents` is `Glyphs | Groups`), carry an optional per-scroll retry/rollback `Policy` on the wire, expose it in Emet (`scroll` takes `name` + optional `policy` + exactly one of `glyphs`/`groups`; lowercase `rollback`/`keep` and a `retry { … }` record), and thread a leaf-unit name-path through golemd's WAL — all while every existing flat `scroll { name, glyphs }` program keeps compiling unchanged.

**Architecture:** The wire model lives in `libs/scroll-format` (the shared writer/reader crate). Plan 1 changes only the data shape and the *paths that carry it*: scroll-format gains `Contents`/`Policy`/`OnExhaust` and bumps `FORMAT_VERSION` 2→3; `emetc` (the `emet` crate) gains the surface constructors, types, and lowering; `golemd` compiles against the new `Scroll` by **flattening every leaf's glyphs** into the existing per-glyph diff (no semantic change to enact/retry/rollback yet — that is Plan 2) while additively carrying a `unit_path` through the WAL step and its sqlite column. Plan 2 layers the per-unit best-effort semantics on top of these types.

**Tech Stack:** Rust (Cargo workspace; `postcard` non-self-describing binary wire, `blake3` content ids, `serde`/`serde_json`, `rusqlite`), the Emet compiler (`chumsky` parser, Algorithm-W inference in `infer.rs`, tree-walking `eval.rs`), Python `typer`/`rich` fleet CLI.

## Global Constraints

- **Zero comments in implementation code.** Every code snippet in this plan is written with no comments; implementers add none. A separate documenter agent owns all comments and prose afterward. Each task ends with a **Doc backlog** line naming what the documenter should later explain.
- **TDD, red-green, every behavior.** Write the failing test first, run it to see it fail for the stated reason, write the minimal implementation, run it green. Use each crate's existing test style and locations (Rust: in-crate `#[cfg(test)] mod tests` and `apps/<crate>/tests/*.rs`; scroll-format golden bytes in `libs/scroll-format/tests/determinism.rs`; Emet suites in `apps/emet/tests/*.rs` with the `common` harness).
- **The wire field/variant order in ADR 0031 §5 is normative.** postcard is non-self-describing — order **is** the encoding. `Scroll` fields: `name: String`, `policy: Option<Policy>`, `contents: Contents`. `Contents` variants: `Glyphs(Vec<Glyph>)`, then `Groups(Vec<Scroll>)`. `Policy` fields in order: `base_delay_ms`, `backoff_multiplier`, `max_delay_ms`, `jitter_fraction`, `max_attempts`, `max_elapsed_ms`, `on_exhaust` (all `Option<…>`). `OnExhaust` variants: `Rollback`, then `Keep`. Do not reorder later without another `format_version` bump.
- **`FORMAT_VERSION` bumps 2 → 3** in this plan (`libs/scroll-format/src/manifest.rs:20`). This is the *same* 2→3 bump ADR 0030 introduces; if ADR 0030's `aptPackage` enrichment lands separately, the two changes share the single bump to 3 — do not bump to 4. The golden bytes in `determinism.rs` change and are regenerated deliberately (Task 3).
- **`main : List Scroll` and existing flat `scroll { name, glyphs = [ … ] }` programs must keep working unchanged.** `glyphs` still names a leaf's contents; a flat scroll is a leaf. This is a hard acceptance test.
- **Build/test commands (verified against the repo — Cargo workspace under a nix `devenv`; `cargo` is on PATH in the dev shell):**
  - Whole workspace: `cargo test --workspace`
  - One crate: `cargo test -p scroll-format` / `cargo test -p emet` / `cargo test -p golemd`
  - One test: `cargo test -p emet <test_name>`
  - Build: `cargo build --workspace`
  - Emet program run (manual check): `cargo run -p emet -- <file.emet>` (`--text` for the readable plan)
  - Fleet smoke compile (manual check): from repo root, `PYTHONPATH=apps python -m fleet` is the harness, but for Plan 1 the relevant fleet check is that `apps/fleet/smoke.emet` and `apps/fleet/reload-proof.emet` still compile via `cargo run -p emet -- apps/fleet/smoke.emet`.
- **Git discipline.** Never touch the `result` symlink; never `git push`. Commit steps use `git add <specific paths>` (never `-A`). Every commit message ends with the trailer line:
  `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`

---

## File Structure

**scroll-format (`libs/scroll-format/`):**
- `src/scroll.rs` — add `Contents`, `Policy`, `OnExhaust`; change `Scroll` to `{ name, policy, contents }`; add leaf-iteration helpers (`glyphs()` accessor, `leaf_units()` yielding `(unit_path, &[Glyph], &Option<Policy>)`); update `describe()`/`key()` (glyph `key()` unchanged).
- `src/manifest.rs` — bump `FORMAT_VERSION` 2→3.
- `src/lib.rs` — re-export the new types.
- `tests/determinism.rs` — new golden bytes + a recursive-tree golden.

**emet (`apps/emet/`):**
- `src/ast.rs` — `Expr::Scroll { name, policy: Option<Box<Spanned<Expr>>>, contents: ContentsExpr }`; new `ContentsExpr` enum; new `Expr` policy constructors.
- `src/ir.rs` — re-export `Contents`, `Policy`, `OnExhaust`.
- `src/parser.rs` — reserve `rollback`/`keep`/`retry`; extend `build_constructor` for `scroll` (glyphs-xor-groups, optional policy) and the policy constructors.
- `src/infer.rs` — type `Expr::Scroll` (contents + optional policy), type the policy constructors; register `Contents`/`Policy`/`OnExhaust` as built-in types.
- `src/eval.rs` — lower the recursive scroll, policy, and contents to scroll-format types; new `Value` handling for policy.
- `src/prelude.rs` — first-class `Policy`/`OnExhaust`/`Contents` types; match-only `OnExhaust` tags if matched (not required by ADR, deferred).
- `tests/scrolls.rs`, `tests/recursive_scroll.rs` (new), `tests/common/mod.rs` — grouping, policy, xor-enforcement, and the flat-still-works guarantee.

**golemd (`apps/golemd/`):**
- `src/reconcile.rs` — flatten leaf glyphs for `plan()`; keep the diff per-glyph.
- `src/journal.rs` — `WalStep` gains `unit_path: Vec<String>`.
- `src/planroom.rs` — `wal_step` table gains a `unit_path` TEXT (serde_json) column; append/read carry it.
- `src/foreman.rs` — thread `unit_path` through `enact`/`enact_apply`/`enact_reverse`/`rollback_attempt` (carried, not yet consulted); `apply_manifest`/`select` handle the recursive `Scroll`.
- `src/wal.rs` — `applied_outcomes` unchanged; `outcome_of`/`next_reversible` unaffected by the new column.

**fleet (`apps/fleet/`):**
- `cli.py` — `_render_revision` / `_glyph_desc` keep working against the (unchanged-in-Plan-1) revision JSON; the `state` view's `scroll.glyphs` access becomes `scroll.contents`.

---

## Task 1: scroll-format — the recursive `Scroll`, `Contents`, `Policy`, `OnExhaust` types

**Files:**
- Modify: `libs/scroll-format/src/scroll.rs`
- Modify: `libs/scroll-format/src/lib.rs`
- Test: `libs/scroll-format/src/scroll.rs` (in-crate `#[cfg(test)] mod tests`)

**Interfaces:**
- Produces (relied on by every later task and by Plan 2):
  - `pub struct Scroll { pub name: String, pub policy: Option<Policy>, pub contents: Contents }`
  - `pub enum Contents { Glyphs(Vec<Glyph>), Groups(Vec<Scroll>) }`
  - `pub struct Policy { pub base_delay_ms: Option<u64>, pub backoff_multiplier: Option<f64>, pub max_delay_ms: Option<u64>, pub jitter_fraction: Option<f64>, pub max_attempts: Option<u32>, pub max_elapsed_ms: Option<u64>, pub on_exhaust: Option<OnExhaust> }`
  - `pub enum OnExhaust { Rollback, Keep }`
  - `impl Scroll { pub fn glyphs(&self) -> &[Glyph] }` — the leaf's glyphs, or `&[]` for a branch.
  - `impl Scroll { pub fn is_leaf(&self) -> bool }`
  - `impl Scroll { pub fn leaf_units(&self) -> Vec<LeafUnit<'_>> }` where `pub struct LeafUnit<'a> { pub path: Vec<String>, pub glyphs: &'a [Glyph], pub policy_chain: Vec<&'a Policy> }` — every leaf in source order, each with its root→leaf name-path and the ancestor-to-leaf policy chain (root-most first, the leaf's own policy last).
  - `impl Scroll { pub fn all_glyphs(&self) -> Vec<&Glyph> }` — every leaf glyph flattened in source order.

- [ ] **Step 1: Write the failing test**

Add to a new `#[cfg(test)] mod tests` block at the end of `libs/scroll-format/src/scroll.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn apt(name: &str) -> Glyph {
        Glyph::AptPackage { name: name.to_string() }
    }

    fn leaf(name: &str, glyphs: Vec<Glyph>) -> Scroll {
        Scroll { name: name.to_string(), policy: None, contents: Contents::Glyphs(glyphs) }
    }

    fn branch(name: &str, groups: Vec<Scroll>) -> Scroll {
        Scroll { name: name.to_string(), policy: None, contents: Contents::Groups(groups) }
    }

    #[test]
    fn leaf_reports_its_glyphs_and_is_a_leaf() {
        let s = leaf("db", vec![apt("postgresql")]);
        assert!(s.is_leaf());
        assert_eq!(s.glyphs(), &[apt("postgresql")]);
    }

    #[test]
    fn branch_has_no_glyphs_and_is_not_a_leaf() {
        let s = branch("host", vec![leaf("db", vec![apt("postgresql")])]);
        assert!(!s.is_leaf());
        assert_eq!(s.glyphs(), &[] as &[Glyph]);
    }

    #[test]
    fn leaf_units_walk_source_order_with_name_paths() {
        let host = branch(
            "worker-01",
            vec![
                branch(
                    "fishnet",
                    vec![leaf("client-1", vec![apt("stockfish")]), leaf("client-2", vec![apt("stockfish")])],
                ),
                leaf("base", vec![apt("htop")]),
            ],
        );
        let units = host.leaf_units();
        let paths: Vec<Vec<String>> = units.iter().map(|u| u.path.clone()).collect();
        assert_eq!(
            paths,
            vec![
                vec!["worker-01".to_string(), "fishnet".to_string(), "client-1".to_string()],
                vec!["worker-01".to_string(), "fishnet".to_string(), "client-2".to_string()],
                vec!["worker-01".to_string(), "base".to_string()],
            ]
        );
        assert_eq!(units[2].glyphs, &[apt("htop")]);
    }

    #[test]
    fn policy_chain_is_root_to_leaf() {
        let child = Scroll {
            name: "client-2".to_string(),
            policy: Some(Policy { on_exhaust: Some(OnExhaust::Keep), ..Policy::default() }),
            contents: Contents::Glyphs(vec![apt("stockfish")]),
        };
        let host = Scroll {
            name: "worker".to_string(),
            policy: Some(Policy { max_attempts: Some(9), ..Policy::default() }),
            contents: Contents::Groups(vec![child]),
        };
        let units = host.leaf_units();
        assert_eq!(units.len(), 1);
        assert_eq!(units[0].policy_chain.len(), 2);
        assert_eq!(units[0].policy_chain[0].max_attempts, Some(9));
        assert_eq!(units[0].policy_chain[1].on_exhaust, Some(OnExhaust::Keep));
    }

    #[test]
    fn all_glyphs_flattens_in_source_order() {
        let host = branch(
            "h",
            vec![leaf("a", vec![apt("one"), apt("two")]), leaf("b", vec![apt("three")])],
        );
        let flat: Vec<&Glyph> = host.all_glyphs();
        assert_eq!(flat, vec![&apt("one"), &apt("two"), &apt("three")]);
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p scroll-format leaf_reports_its_glyphs_and_is_a_leaf`
Expected: compile error — `Contents`, `Policy`, `OnExhaust`, `Policy::default`, `is_leaf`, `glyphs`, `leaf_units`, `all_glyphs` do not exist, and `Scroll` has no `policy`/`contents` fields.

- [ ] **Step 3: Replace the `Scroll` struct and add the new types**

In `libs/scroll-format/src/scroll.rs`, replace the existing `Scroll` struct (currently `pub struct Scroll { pub name: String, pub glyphs: Vec<Glyph> }` at lines 90–94) and its `impl Scroll { describe }` block (lines 96–102) with:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scroll {
    pub name: String,
    pub policy: Option<Policy>,
    pub contents: Contents,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Contents {
    Glyphs(Vec<Glyph>),
    Groups(Vec<Scroll>),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Policy {
    pub base_delay_ms: Option<u64>,
    pub backoff_multiplier: Option<f64>,
    pub max_delay_ms: Option<u64>,
    pub jitter_fraction: Option<f64>,
    pub max_attempts: Option<u32>,
    pub max_elapsed_ms: Option<u64>,
    pub on_exhaust: Option<OnExhaust>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OnExhaust {
    Rollback,
    Keep,
}

pub struct LeafUnit<'a> {
    pub path: Vec<String>,
    pub glyphs: &'a [Glyph],
    pub policy_chain: Vec<&'a Policy>,
}

impl Scroll {
    pub fn is_leaf(&self) -> bool {
        matches!(self.contents, Contents::Glyphs(_))
    }

    pub fn glyphs(&self) -> &[Glyph] {
        match &self.contents {
            Contents::Glyphs(g) => g,
            Contents::Groups(_) => &[],
        }
    }

    pub fn all_glyphs(&self) -> Vec<&Glyph> {
        let mut out = Vec::new();
        self.collect_glyphs(&mut out);
        out
    }

    fn collect_glyphs<'a>(&'a self, out: &mut Vec<&'a Glyph>) {
        match &self.contents {
            Contents::Glyphs(g) => out.extend(g.iter()),
            Contents::Groups(children) => {
                for child in children {
                    child.collect_glyphs(out);
                }
            }
        }
    }

    pub fn leaf_units(&self) -> Vec<LeafUnit<'_>> {
        let mut out = Vec::new();
        self.collect_leaves(&mut Vec::new(), &mut Vec::new(), &mut out);
        out
    }

    fn collect_leaves<'a>(
        &'a self,
        path: &mut Vec<String>,
        policy_chain: &mut Vec<&'a Policy>,
        out: &mut Vec<LeafUnit<'a>>,
    ) {
        path.push(self.name.clone());
        if let Some(p) = &self.policy {
            policy_chain.push(p);
        }
        match &self.contents {
            Contents::Glyphs(g) => out.push(LeafUnit {
                path: path.clone(),
                glyphs: g,
                policy_chain: policy_chain.clone(),
            }),
            Contents::Groups(children) => {
                for child in children {
                    child.collect_leaves(path, policy_chain, out);
                }
            }
        }
        if self.policy.is_some() {
            policy_chain.pop();
        }
        path.pop();
    }

    pub fn describe(&self) -> String {
        match &self.contents {
            Contents::Glyphs(g) => format!("scroll `{}` ({} glyphs)", self.name, g.len()),
            Contents::Groups(children) => {
                format!("scroll `{}` ({} groups)", self.name, children.len())
            }
        }
    }
}
```

- [ ] **Step 4: Re-export the new types**

In `libs/scroll-format/src/lib.rs`, change the `pub use scroll::{…}` line (line 29) from:

```rust
pub use scroll::{Entry, Glyph, Perms, Scroll};
```

to:

```rust
pub use scroll::{Contents, Entry, Glyph, LeafUnit, OnExhaust, Perms, Policy, Scroll};
```

- [ ] **Step 5: Run the new tests to verify they pass**

Run: `cargo test -p scroll-format`
Expected: the four new tests pass. The `determinism.rs` and `manifest.rs` tests **fail to compile** now (they build `Scroll { name, glyphs }`) — that is expected and fixed in Task 2 and Task 3. To run only the in-crate tests while the integration test is red, use: `cargo test -p scroll-format --lib`
Expected: PASS for the `scroll::tests` module.

- [ ] **Step 6: Commit**

```bash
git add libs/scroll-format/src/scroll.rs libs/scroll-format/src/lib.rs
git commit -m "feat(scroll-format): make Scroll a recursive tree with Policy

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** explain (a) why `Contents` is a sum (strict tree, illegal mixed levels unrepresentable, ADR 0031 §1); (b) the field/variant order is the wire encoding; (c) `leaf_units`/`policy_chain` semantics (root→leaf, nearest-wins is resolved by the consumer); (d) that `glyphs()` returns `&[]` for a branch by design.

---

## Task 2: scroll-format — bump `FORMAT_VERSION` to 3 and fix `manifest.rs`

**Files:**
- Modify: `libs/scroll-format/src/manifest.rs`
- Test: `libs/scroll-format/tests/determinism.rs` (the `unknown_format_version_is_a_clean_error` and `manifest_round_trips_through_bytes` tests already exist; they must pass once bytes are regenerated in Task 3).

**Interfaces:**
- Consumes: the types from Task 1.
- Produces: `pub const FORMAT_VERSION: u32 = 3;`

- [ ] **Step 1: Write the failing test**

Add to `libs/scroll-format/src/manifest.rs` a new in-crate test module (there is none today):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_version_is_three() {
        assert_eq!(FORMAT_VERSION, 3);
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p scroll-format --lib format_version_is_three`
Expected: FAIL — `assert_eq!(2, 3)`.

- [ ] **Step 3: Bump the constant**

In `libs/scroll-format/src/manifest.rs`, change line 20 from `pub const FORMAT_VERSION: u32 = 2;` to `pub const FORMAT_VERSION: u32 = 3;`.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p scroll-format --lib format_version_is_three`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add libs/scroll-format/src/manifest.rs
git commit -m "feat(scroll-format): bump FORMAT_VERSION 2 to 3 for recursive Scroll

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** the NOTE comment at `manifest.rs:16-19` describing *why* the version is what it is must be rewritten to cite the recursive `Scroll` (and, if ADR 0030 landed, the `aptPackage` enrichment sharing the same bump). Explain a v2 manifest cleanly fails `check_format_version` rather than misparsing.

---

## Task 3: scroll-format — regenerate golden bytes and add a recursive-tree golden

**Files:**
- Modify: `libs/scroll-format/tests/determinism.rs`
- Test: same file.

**Interfaces:**
- Consumes: Task 1 types, Task 2 `FORMAT_VERSION`.

- [ ] **Step 1: Update the fixtures to the recursive shape**

In `libs/scroll-format/tests/determinism.rs`, update the imports and the two fixture constructors. Change the import line (lines 1–4) to add `Contents`:

```rust
use scroll_format::{
    content_id, content_id_of_glyph, from_bytes, to_bytes, Contents, ContentId, Entry,
    FromBytesError, Glyph, Manifest, Perms, Scroll, FORMAT_VERSION,
};
```

Replace `fixed_scroll` (lines 6–33) so its `glyphs: vec![…]` becomes `policy: None, contents: Contents::Glyphs(vec![…])`:

```rust
fn fixed_scroll() -> Scroll {
    Scroll {
        name: "web".to_string(),
        policy: None,
        contents: Contents::Glyphs(vec![
            Glyph::AptPackage { name: "nginx".to_string() },
            Glyph::SystemdService { unit: "nginx.service".to_string() },
            Glyph::Filesystem {
                path: "/etc/nginx/nginx.conf".to_string(),
                entry: Entry::File {
                    contents: "worker_processes auto;".to_string(),
                    perms: Perms { mode: 0o644, owner: None, group: None },
                },
            },
            Glyph::LineInFile {
                path: "/etc/hosts".to_string(),
                line: "127.0.0.1 localhost".to_string(),
            },
        ]),
    }
}
```

Replace `other_scroll` (lines 35–42):

```rust
fn other_scroll() -> Scroll {
    Scroll {
        name: "db".to_string(),
        policy: None,
        contents: Contents::Glyphs(vec![Glyph::AptPackage {
            name: "postgresql".to_string(),
        }]),
    }
}
```

- [ ] **Step 2: Turn the golden-byte tests into a regeneration harness (temporarily)**

Replace the two golden constants and `fixed_scroll_serializes_to_golden_bytes` / `fixed_scroll_hashes_to_constant_content_id` (lines 44–66) with a temporary printing test so the new canonical bytes can be captured:

```rust
#[test]
fn print_golden() {
    let bytes = postcard::to_stdvec(&fixed_scroll()).unwrap();
    println!("BYTES={bytes:?}");
    println!("CID={}", content_id(&fixed_scroll()));
    panic!("capture golden values");
}
```

- [ ] **Step 3: Run the printing test and capture the values**

Run: `cargo test -p scroll-format --test determinism print_golden -- --nocapture`
Expected: FAIL (the deliberate `panic!`), with `BYTES=[…]` and `CID=…` printed. Copy the exact `[…]` byte list and the 64-hex CID.

- [ ] **Step 4: Bake the captured values back as golden constants**

Replace the `print_golden` test with the restored golden test, pasting the captured values into `GOLDEN_SCROLL_BYTES` and `GOLDEN_CONTENT_ID`:

```rust
const GOLDEN_SCROLL_BYTES: &[u8] = &[ /* paste the captured byte list here */ ];

const GOLDEN_CONTENT_ID: &str = "/* paste the captured 64-hex content id here */";

#[test]
fn fixed_scroll_serializes_to_golden_bytes() {
    let bytes = postcard::to_stdvec(&fixed_scroll()).unwrap();
    assert_eq!(bytes, GOLDEN_SCROLL_BYTES);
}

#[test]
fn fixed_scroll_hashes_to_constant_content_id() {
    let id = content_id(&fixed_scroll());
    assert_eq!(id.to_string(), GOLDEN_CONTENT_ID);
}
```

- [ ] **Step 5: Add a recursive-tree determinism test**

Append to `libs/scroll-format/tests/determinism.rs`:

```rust
fn nested_host() -> Scroll {
    Scroll {
        name: "worker-01".to_string(),
        policy: None,
        contents: Contents::Groups(vec![
            Scroll {
                name: "fishnet".to_string(),
                policy: None,
                contents: Contents::Groups(vec![Scroll {
                    name: "client-1".to_string(),
                    policy: None,
                    contents: Contents::Glyphs(vec![Glyph::AptPackage {
                        name: "stockfish".to_string(),
                    }]),
                }]),
            },
            Scroll {
                name: "base".to_string(),
                policy: None,
                contents: Contents::Glyphs(vec![Glyph::AptPackage { name: "htop".to_string() }]),
            },
        ]),
    }
}

#[test]
fn nested_scroll_round_trips_through_a_manifest() {
    let manifest = Manifest::from_scrolls(vec![nested_host()], "0.1.0");
    let decoded = from_bytes(&to_bytes(&manifest)).unwrap();
    assert_eq!(decoded, manifest);
    assert_eq!(decoded.format_version, FORMAT_VERSION);
}

#[test]
fn a_leaf_glyph_content_id_is_independent_of_grouping() {
    let flat = Scroll {
        name: "h".to_string(),
        policy: None,
        contents: Contents::Glyphs(vec![Glyph::AptPackage { name: "stockfish".to_string() }]),
    };
    let grouped = nested_host();
    let g_flat = content_id_of_glyph(flat.all_glyphs()[0]);
    let g_grouped = content_id_of_glyph(
        grouped.all_glyphs().into_iter().find(|g| matches!(g, Glyph::AptPackage { name } if name == "stockfish")).unwrap(),
    );
    assert_eq!(g_flat, g_grouped);
}
```

- [ ] **Step 6: Run the full crate test suite**

Run: `cargo test -p scroll-format`
Expected: PASS — golden bytes match the regenerated values, the manifest round-trips, the recursive golden round-trips, `unknown_format_version_is_a_clean_error` still holds (it uses `FORMAT_VERSION + 1`), and the grouping-invariance test passes.

- [ ] **Step 7: Commit**

```bash
git add libs/scroll-format/tests/determinism.rs
git commit -m "test(scroll-format): regenerate golden bytes for recursive Scroll v3

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** note that the golden bytes were deliberately regenerated for the v3 wire shape (not a silent drift), and why a per-glyph content id is invariant to grouping (ADR 0031 §4).

---

## Task 4: emet — AST for the recursive scroll and policy

**Files:**
- Modify: `apps/emet/src/ast.rs`
- Modify: `apps/emet/src/ir.rs`
- Test: none directly (AST is exercised through the parser in Task 5); this task must at least `cargo build -p emet` after the parser/infer/eval are updated. To keep it independently testable, this task is folded into Task 5's red-green cycle. It is listed separately only to document the AST shape the later tasks consume.

**Interfaces:**
- Consumes: Task 1 `Contents`/`Policy`/`OnExhaust` (re-exported through `ir.rs`).
- Produces:
  - `Expr::Scroll { name: Box<Spanned<Expr>>, policy: Option<Box<Spanned<Expr>>>, contents: ContentsExpr }`
  - `pub enum ContentsExpr { Glyphs(Box<Spanned<Expr>>), Groups(Box<Spanned<Expr>>) }`
  - `Expr::PolicyExhaust(OnExhaustTag)` where `pub enum OnExhaustTag { Rollback, Keep }`
  - `Expr::PolicyRetry(BTreeMap<String, Spanned<Expr>>)` — the raw `retry { … }` fields, resolved in eval.

- [ ] **Step 1: Extend the AST**

In `apps/emet/src/ast.rs`, replace the `Scroll` variant of `enum Expr` (lines 186–192) with:

```rust
    Scroll {
        name: Box<Spanned<Expr>>,
        policy: Option<Box<Spanned<Expr>>>,
        contents: ContentsExpr,
    },
    PolicyExhaust(OnExhaustTag),
    PolicyRetry(BTreeMap<String, Spanned<Expr>>),
```

After the `EntryExpr` enum (after line 253), add:

```rust
#[derive(Debug, Clone)]
pub enum ContentsExpr {
    Glyphs(Box<Spanned<Expr>>),
    Groups(Box<Spanned<Expr>>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnExhaustTag {
    Rollback,
    Keep,
}
```

Ensure `use std::collections::BTreeMap;` is present in `ast.rs` (the `Record` variant already uses it, so it is).

- [ ] **Step 2: Re-export the scroll-format policy types through the IR**

In `apps/emet/src/ir.rs`, change line 7 from:

```rust
pub use scroll_format::{Entry, Glyph, Perms, Scroll};
```

to:

```rust
pub use scroll_format::{Contents, Entry, Glyph, OnExhaust, Perms, Policy, Scroll};
```

- [ ] **Step 3: Build to confirm the AST compiles**

Run: `cargo build -p emet`
Expected: compile errors in `parser.rs`, `infer.rs`, `eval.rs` (all match on `Expr::Scroll { name, glyphs }` — the old shape). These are fixed in Tasks 5–7. This task has no standalone green; it commits together with Task 5.

**Doc backlog:** document that `ContentsExpr` mirrors scroll-format's `Contents` at the surface and that `PolicyExhaust`/`PolicyRetry` are the two spellings of a policy (the `rollback`/`keep` shorthand vs. the full `retry { … }` record).

---

## Task 5: emet — parser for `scroll` (glyphs xor groups, optional policy) and the policy constructors

**Files:**
- Modify: `apps/emet/src/parser.rs`
- Test: `apps/emet/tests/recursive_scroll.rs` (new)

**Interfaces:**
- Consumes: Task 4 AST (`Expr::Scroll`, `ContentsExpr`, `Expr::PolicyExhaust`, `Expr::PolicyRetry`, `OnExhaustTag`).
- Produces: the parser accepts `scroll { name, glyphs }`, `scroll { name, groups }`, `scroll { name, policy, glyphs|groups }`, `rollback`, `keep`, and `retry { … }`; rejects both-or-neither of glyphs/groups with a specific diagnostic.

- [ ] **Step 1: Write the failing tests**

Create `apps/emet/tests/recursive_scroll.rs`:

```rust
mod common;

use common::err;
use emet::{compile, ir::Contents, ir::Glyph, ir::OnExhaust, ir::Scroll, Phase};

fn scrolls(src: &str) -> Vec<Scroll> {
    match compile(src) {
        Ok(c) => c.scrolls,
        Err(e) => panic!("expected success, got {:?}: {}", e.phase, e.msg),
    }
}

#[test]
fn flat_scroll_still_lowers_to_a_leaf() {
    let src = r#"main = [ scroll { name = "db", glyphs = [ aptPackage { name = "postgresql" } ] } ]"#;
    let ss = scrolls(src);
    assert_eq!(ss.len(), 1);
    assert!(ss[0].is_leaf());
    assert_eq!(ss[0].glyphs(), &[Glyph::AptPackage { name: "postgresql".into() }]);
    assert_eq!(ss[0].policy, None);
}

#[test]
fn groups_build_a_branch_tree_in_source_order() {
    let src = r#"
main =
  [ scroll { name = "worker", groups =
      [ scroll { name = "a", glyphs = [ aptPackage { name = "one" } ] }
      , scroll { name = "b", glyphs = [ aptPackage { name = "two" } ] }
      ] }
  ]
"#;
    let ss = scrolls(src);
    assert_eq!(ss.len(), 1);
    match &ss[0].contents {
        Contents::Groups(children) => {
            assert_eq!(children.len(), 2);
            assert_eq!(children[0].name, "a");
            assert_eq!(children[1].name, "b");
        }
        Contents::Glyphs(_) => panic!("expected groups"),
    }
    let units = ss[0].leaf_units();
    assert_eq!(units[0].path, vec!["worker".to_string(), "a".to_string()]);
}

#[test]
fn scroll_with_both_glyphs_and_groups_is_a_parse_error() {
    let src = r#"main = [ scroll { name = "x", glyphs = [ ], groups = [ ] } ]"#;
    let e = err(src);
    assert_eq!(e.phase, Phase::Parse);
    assert!(e.msg.contains("exactly one of `glyphs` or `groups`"), "got: {}", e.msg);
}

#[test]
fn scroll_with_neither_glyphs_nor_groups_is_a_parse_error() {
    let src = r#"main = [ scroll { name = "x" } ]"#;
    let e = err(src);
    assert_eq!(e.phase, Phase::Parse);
    assert!(e.msg.contains("exactly one of `glyphs` or `groups`"), "got: {}", e.msg);
}

#[test]
fn keep_policy_lowers_to_on_exhaust_keep() {
    let src = r#"main = [ scroll { name = "x", policy = keep, glyphs = [ aptPackage { name = "one" } ] } ]"#;
    let ss = scrolls(src);
    let policy = ss[0].policy.clone().expect("policy present");
    assert_eq!(policy.on_exhaust, Some(OnExhaust::Keep));
    assert_eq!(policy.max_attempts, None);
}

#[test]
fn rollback_policy_lowers_to_on_exhaust_rollback() {
    let src = r#"main = [ scroll { name = "x", policy = rollback, glyphs = [ aptPackage { name = "one" } ] } ]"#;
    let ss = scrolls(src);
    assert_eq!(ss[0].policy.clone().unwrap().on_exhaust, Some(OnExhaust::Rollback));
}

#[test]
fn retry_record_sets_the_knobs() {
    let src = r#"
main =
  [ scroll
      { name = "x"
      , policy = retry { maxAttempts = 3, baseDelayMs = 500, backoffMultiplier = 2.0, onExhaust = keep }
      , glyphs = [ aptPackage { name = "one" } ]
      }
  ]
"#;
    let policy = scrolls(src)[0].policy.clone().unwrap();
    assert_eq!(policy.max_attempts, Some(3));
    assert_eq!(policy.base_delay_ms, Some(500));
    assert_eq!(policy.backoff_multiplier, Some(2.0));
    assert_eq!(policy.on_exhaust, Some(OnExhaust::Keep));
    assert_eq!(policy.jitter_fraction, None);
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p emet --test recursive_scroll`
Expected: compile failure (the crate does not yet build with the new AST) or, once it builds, the new tests fail. First get the crate compiling via Steps 3–7, then this suite goes green.

- [ ] **Step 3: Reserve the three new constructor words**

In `apps/emet/src/parser.rs`, extend `is_reserved_constructor` (lines 1009–1014):

```rust
fn is_reserved_constructor(name: &str) -> bool {
    matches!(
        name,
        "aptPackage"
            | "systemdService"
            | "file"
            | "directory"
            | "symlink"
            | "lineInFile"
            | "scroll"
            | "rollback"
            | "keep"
            | "retry"
    )
}
```

- [ ] **Step 4: Handle `rollback`/`keep` as zero-field constructors and `retry` as a record constructor**

`rollback` and `keep` are written without braces (`policy = keep`), so they must parse as bare atoms, not `name { … }`. Add a dedicated atom before the `constructor` atom. In the `parse_expr` builder (near line 371 where `constructor_name` is defined), add:

```rust
        let policy_word = select! {
            Tok::Ident(name) if name == "rollback" || name == "keep" => name,
        }
        .map_with(|name, e| {
            let tag = if name == "rollback" {
                crate::ast::OnExhaustTag::Rollback
            } else {
                crate::ast::OnExhaustTag::Keep
            };
            Spanned(Expr::PolicyExhaust(tag), span_range(e.span()))
        });
```

Add `policy_word` to the `choice((…))` atom list (line 449), placed **before** `var` and `constructor` so the reserved words dispatch to it:

```rust
        let atom = choice((interpolated, str_lit, char_lit, float_lit, int_lit, policy_word, constructor, var, qualified, ctor, paren, list, record));
```

- [ ] **Step 5: Extend `build_constructor` for `retry` and the recursive `scroll`**

In `apps/emet/src/parser.rs`, replace the `"scroll"` arm of `build_constructor` (lines 1061–1064) and add a `"retry"` arm. Also `rollback`/`keep` never reach `build_constructor` (they are handled by `policy_word`), so add explicit arms that error if braces are used. The full changed section of `build_constructor` (inside the `match ctor { … }`):

```rust
        "scroll" => {
            let name = Box::new(take_field(ctor, fields, "name", span)?);
            let policy = fields.remove("policy").map(Box::new);
            let glyphs = fields.remove("glyphs");
            let groups = fields.remove("groups");
            let contents = match (glyphs, groups) {
                (Some(g), None) => ContentsExpr::Glyphs(Box::new(g)),
                (None, Some(g)) => ContentsExpr::Groups(Box::new(g)),
                _ => {
                    return Err(Rich::custom(
                        span,
                        "`scroll` needs exactly one of `glyphs` or `groups`".to_string(),
                    ))
                }
            };
            Expr::Scroll { name, policy, contents }
        }
        "retry" => Expr::PolicyRetry(std::mem::take(fields)),
        "rollback" | "keep" => {
            return Err(Rich::custom(
                span,
                format!("`{ctor}` is written without braces (e.g. `policy = {ctor}`)"),
            ))
        }
```

Add the AST imports at the top of `parser.rs` where `EntryExpr` is already imported. Find the existing `use crate::ast::{… EntryExpr …};` and add `ContentsExpr`:

```rust
use crate::ast::{/* existing items */, ContentsExpr, EntryExpr, Expr, Spanned, Type};
```

(Match the exact existing `use crate::ast::…` line and add `ContentsExpr` to it; `Expr`/`Spanned` are already imported.)

Note the leftover-field check at lines 1067–1069 still runs for `scroll` (unknown fields like `ipv4` are still rejected) — `name`/`policy`/`glyphs`/`groups` are all removed before it. For `retry`, `std::mem::take(fields)` empties the map so the leftover check passes; unknown retry fields are validated in inference (Task 6).

- [ ] **Step 6: Run the crate build**

Run: `cargo build -p emet`
Expected: `parser.rs` compiles; remaining errors are only in `infer.rs` and `eval.rs` (fixed in Tasks 6–7).

- [ ] **Step 7: (blocked)** — the `recursive_scroll` suite cannot pass until infer (Task 6) and eval (Task 7) land. Do not commit yet; Tasks 5–7 commit together in Task 7 Step 5, after the suite is green. (This preserves red-green: the test file exists and is red now, green after Task 7.)

**Doc backlog:** explain the glyphs-xor-groups enforcement mirrors the filesystem glyph's per-arm field discipline (ADR 0019/0031 §7); that `rollback`/`keep` are braceless build words and `retry { … }` is the full record; and why the diagnostic wording is exact (a test asserts on "exactly one of `glyphs` or `groups`").

---

## Task 6: emet — inference for the recursive scroll and policy

**Files:**
- Modify: `apps/emet/src/infer.rs`
- Modify: `apps/emet/src/prelude.rs`
- Test: `apps/emet/tests/recursive_scroll.rs` (type-level cases), extended.

**Interfaces:**
- Consumes: Task 4 AST, Task 5 parser output.
- Produces: `Scroll`, `Policy`, `OnExhaust`, `Contents` are first-class built-in types; `Expr::Scroll`'s `groups` field must be `List Scroll`, `glyphs` must be `List Glyph`, `policy` must be `Policy`; `rollback`/`keep`/`retry { … }` infer as `Policy`.

- [ ] **Step 1: Add type-checking tests**

Append to `apps/emet/tests/recursive_scroll.rs`:

```rust
#[test]
fn groups_must_be_a_scroll_list_not_a_glyph_list() {
    let src = r#"main = [ scroll { name = "x", groups = [ aptPackage { name = "one" } ] } ]"#;
    let e = err(src);
    assert_eq!(e.phase, Phase::Type);
}

#[test]
fn policy_field_must_be_a_policy() {
    let src = r#"main = [ scroll { name = "x", policy = "nope", glyphs = [ ] } ]"#;
    let e = err(src);
    assert_eq!(e.phase, Phase::Type);
}

#[test]
fn retry_backoff_multiplier_must_be_a_float() {
    let src = r#"main = [ scroll { name = "x", policy = retry { backoffMultiplier = "nope" }, glyphs = [ ] } ]"#;
    let e = err(src);
    assert_eq!(e.phase, Phase::Type);
}

#[test]
fn a_library_can_annotate_a_policy_value() {
    let src = r#"
p : Policy
p = retry { maxAttempts = 4 }

main = [ scroll { name = "x", policy = p, glyphs = [ aptPackage { name = "one" } ] } ]
"#;
    let ss = scrolls(src);
    assert_eq!(ss[0].policy.clone().unwrap().max_attempts, Some(4));
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p emet --test recursive_scroll a_library_can_annotate_a_policy_value`
Expected: compile failure or type error — `Policy` is not yet a known type constructor and `Expr::Scroll`/`Expr::PolicyRetry`/`Expr::PolicyExhaust` are not inferred.

- [ ] **Step 3: Register the new built-in types**

In `apps/emet/src/infer.rs`, add `Policy`, `OnExhaust`, `Contents` to `builtin_type_arity` (lines 1738–1745):

```rust
fn builtin_type_arity(name: &str) -> Option<usize> {
    match name {
        "String" | "Char" | "AptPackage" | "SystemdService" | "Filesystem" | "LineInFile"
        | "Glyph" | "Entry" | "Scroll" | "Policy" | "OnExhaust" | "Contents" | "Bool" | "Int"
        | "Float" | "Order" => Some(0),
        "List" | "Maybe" => Some(1),
        _ => None,
    }
}
```

And to `builtin_types` (lines 1936–1956), add the same three names to the arity-0 list:

```rust
    [
        "String",
        "Char",
        "AptPackage",
        "SystemdService",
        "Filesystem",
        "LineInFile",
        "Glyph",
        "Entry",
        "Scroll",
        "Policy",
        "OnExhaust",
        "Contents",
        "Bool",
        "Int",
        "Float",
        "Order",
    ]
```

- [ ] **Step 4: Infer `Expr::Scroll`, `Expr::PolicyExhaust`, `Expr::PolicyRetry`**

In `apps/emet/src/infer.rs`, replace the `Expr::Scroll { name, glyphs }` arm (lines 1086–1094) with:

```rust
        Expr::Scroll { name, policy, contents } => {
            let nt = infer_expr(inf, env, name)?;
            inf.unify(&nt, &con("String"), &name.1)?;
            if let Some(p) = policy {
                let pt = infer_expr(inf, env, p)?;
                inf.unify(&pt, &con("Policy"), &p.1)?;
            }
            match contents {
                ContentsExpr::Glyphs(glyphs) => {
                    let gt = infer_expr(inf, env, glyphs)?;
                    let gt = widen_glyph_subtype(inf, &gt);
                    let glyph_list = Type::Con("List".to_string(), vec![con("Glyph")]);
                    inf.unify(&gt, &glyph_list, &glyphs.1)?;
                }
                ContentsExpr::Groups(groups) => {
                    let gt = infer_expr(inf, env, groups)?;
                    let scroll_list = Type::Con("List".to_string(), vec![con("Scroll")]);
                    inf.unify(&gt, &scroll_list, &groups.1)?;
                }
            }
            Ok(con("Scroll"))
        }

        Expr::PolicyExhaust(_) => Ok(con("Policy")),

        Expr::PolicyRetry(fields) => {
            for (key, value) in fields.iter() {
                let expected = match key.as_str() {
                    "maxAttempts" => con("Int"),
                    "baseDelayMs" | "maxDelayMs" | "maxElapsedMs" => con("Int"),
                    "backoffMultiplier" | "jitterFraction" => con("Float"),
                    "onExhaust" => con("Policy"),
                    other => {
                        return Err(TypeError::new(
                            format!("unknown `retry` field `{other}`"),
                            value.1.clone(),
                        ))
                    }
                };
                let vt = infer_expr(inf, env, value)?;
                inf.unify(&vt, &expected, &value.1)?;
            }
            Ok(con("Policy"))
        }
```

Add `ContentsExpr` to the `use crate::ast::{…}` import at the top of `infer.rs` (it already imports `Expr`, `EntryExpr`, etc.). Match the existing `use crate::ast::` line and add `ContentsExpr`.

Note: `onExhaust`'s value is `keep`/`rollback`, which infer as `Policy` (via `Expr::PolicyExhaust`), so it unifies against `con("Policy")` — this is the pragmatic typing (an `onExhaust` field carries a policy-shorthand value). The eval stage (Task 7) reads the tag directly from the value.

- [ ] **Step 5: Register `Policy`/`OnExhaust`/`Contents` as prelude type builders (for helper reuse)**

In `apps/emet/src/prelude.rs`, add type-builder helpers beside `glyph()`/`entry()` (after line 107):

```rust
fn policy() -> Type {
    Type::Con("Policy".to_string(), vec![])
}
```

This is used only if later tasks need a `Policy`-returning scheme; it is added now so Plan 2 (and any policy-computing library) can reference it. No constructor is registered — `rollback`/`keep`/`retry` are parser-special-cased build words, exactly as `aptPackage`/`file` are (they are *not* in `ctors()`/`ty_env`).

- [ ] **Step 6: Run the type tests**

Run: `cargo test -p emet --test recursive_scroll groups_must_be_a_scroll_list_not_a_glyph_list policy_field_must_be_a_policy retry_backoff_multiplier_must_be_a_float`
Expected: the crate now builds through inference; eval is still stubbed on the old shape so full compiles may still fail. If eval has not been touched, `cargo build -p emet` will error in `eval.rs` — proceed to Task 7 before running the parse/eval-dependent tests. The pure type-error tests (which fail during the `Type` phase before eval) should already pass once inference is in place.

**Doc backlog:** document that `Policy`/`OnExhaust`/`Contents` are first-class types so a library can compute a policy/group tree; that `onExhaust`'s value is a policy-shorthand (`keep`/`rollback`) typed as `Policy` for uniformity; and the unknown-`retry`-field diagnostic.

---

## Task 7: emet — evaluation lowering to the recursive scroll-format `Scroll`

**Files:**
- Modify: `apps/emet/src/eval.rs`
- Test: `apps/emet/tests/recursive_scroll.rs` (all cases now green).

**Interfaces:**
- Consumes: Task 1 scroll-format types (via `ir.rs`), Task 4 AST, Task 6 inference.
- Produces: `Value::Scroll(Scroll { name, policy, contents })` and a `Value::Policy(Policy)` runtime value for policy expressions; the lowering helpers `as_scroll`/`as_glyphs`/`as_policy`.

- [ ] **Step 1: Add a `Policy` runtime value**

In `apps/emet/src/eval.rs`, add a variant to the `Value` enum (lines 34–59), after `Scroll(Scroll)`:

```rust
    Policy(Policy),
```

Add `Policy` to the imports from `ir` at line 13:

```rust
use crate::ir::{Contents, Entry, Glyph, OnExhaust, Perms, Policy, Scroll};
```

Add a `Debug` arm for it in the manual `impl Debug for Value` (near line 84, beside the `Scroll` arm):

```rust
            Value::Policy(p) => f.debug_tuple("Policy").field(p).finish(),
```

- [ ] **Step 2: Lower `Expr::Scroll`, `Expr::PolicyExhaust`, `Expr::PolicyRetry`**

Replace the `Expr::Scroll { name, glyphs }` arm in `eval` (lines 167–170) with:

```rust
        Expr::Scroll { name, policy, contents } => {
            let name = as_str(eval(env, name, depth)?);
            let policy = match policy {
                Some(p) => Some(as_policy(eval(env, p, depth)?)),
                None => None,
            };
            let contents = match contents {
                ContentsExpr::Glyphs(glyphs) => Contents::Glyphs(as_glyphs(eval(env, glyphs, depth)?)),
                ContentsExpr::Groups(groups) => Contents::Groups(as_scrolls(eval(env, groups, depth)?)),
            };
            Value::Scroll(Scroll { name, policy, contents })
        }
        Expr::PolicyExhaust(tag) => Value::Policy(Policy {
            on_exhaust: Some(match tag {
                crate::ast::OnExhaustTag::Rollback => OnExhaust::Rollback,
                crate::ast::OnExhaustTag::Keep => OnExhaust::Keep,
            }),
            ..Policy::default()
        }),
        Expr::PolicyRetry(fields) => {
            let mut policy = Policy::default();
            for (key, value) in fields.iter() {
                let v = eval(env, value, depth)?;
                match key.as_str() {
                    "maxAttempts" => policy.max_attempts = Some(as_int(v) as u32),
                    "baseDelayMs" => policy.base_delay_ms = Some(as_int(v) as u64),
                    "maxDelayMs" => policy.max_delay_ms = Some(as_int(v) as u64),
                    "maxElapsedMs" => policy.max_elapsed_ms = Some(as_int(v) as u64),
                    "backoffMultiplier" => policy.backoff_multiplier = Some(as_float(v)),
                    "jitterFraction" => policy.jitter_fraction = Some(as_float(v)),
                    "onExhaust" => policy.on_exhaust = as_policy(v).on_exhaust,
                    other => unreachable!("unknown retry field {other} survived inference"),
                }
            }
            Value::Policy(policy)
        }
```

Add `ContentsExpr` to the `use crate::ast::{…}` import in `eval.rs` (it already imports `EntryExpr`, `Expr`, etc.); add `ContentsExpr` to that list.

- [ ] **Step 3: Add the lowering helpers**

In `apps/emet/src/eval.rs`, beside `as_glyphs`/`as_glyph`/`as_scroll` (lines 599–618), add:

```rust
fn as_scrolls(v: Value) -> Vec<Scroll> {
    match v {
        Value::List(items) => items.iter().map(as_scroll).collect(),
        _ => unreachable!("expected List of Scroll"),
    }
}

fn as_policy(v: Value) -> Policy {
    match v {
        Value::Policy(p) => p,
        _ => unreachable!("expected Policy"),
    }
}
```

If `as_int` / `as_float` do not already exist in `eval.rs`, add them beside `as_str`:

```rust
fn as_int(v: Value) -> i64 {
    match v {
        Value::Int(n) => n,
        _ => unreachable!("expected Int"),
    }
}

fn as_float(v: Value) -> f64 {
    match v {
        Value::Float(x) => x,
        _ => unreachable!("expected Float"),
    }
}
```

(Check for existing `as_int`/`as_float` first — `perms_from_mode` parses a string mode, so an `as_int` may not exist; add only what is missing.)

- [ ] **Step 4: Run the full recursive-scroll suite and the existing scrolls suite**

Run: `cargo test -p emet --test recursive_scroll`
Expected: every test in the file passes.

Run: `cargo test -p emet --test scrolls`
Expected: **fails to compile** — `scrolls.rs` and `common/mod.rs` assert on `Scroll { name, glyphs }` and `.glyphs`. These are updated in Task 8. If you want a green checkpoint now, temporarily skip; Task 8 fixes the harness and existing suites.

- [ ] **Step 5: Commit Tasks 4–7 together**

```bash
git add apps/emet/src/ast.rs apps/emet/src/ir.rs apps/emet/src/parser.rs apps/emet/src/infer.rs apps/emet/src/eval.rs apps/emet/src/prelude.rs apps/emet/tests/recursive_scroll.rs
git commit -m "feat(emet): recursive scroll surface with optional policy

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** document the lowering: policy expressions evaluate to `Value::Policy`, contents to `Contents::Glyphs|Groups`; `retry`'s `onExhaust` reuses the policy-shorthand value; the `unreachable!` arms are guarded by inference (they cannot fire on a type-checked program).

---

## Task 8: emet — fix the existing test harness and suites for the new `Scroll` shape

**Files:**
- Modify: `apps/emet/tests/common/mod.rs`
- Modify: `apps/emet/tests/scrolls.rs`
- Modify: `apps/emet/tests/lichess.rs`
- Modify: `apps/emet/tests/quadlet.rs`
- Modify: `apps/emet/tests/modules.rs`
- Modify: `apps/emet/tests/library_search_path.rs`
- Modify: `apps/emet/src/main.rs`, `apps/emet/src/lib.rs` (any `.glyphs`/`Scroll {` uses)
- Test: all emet suites.

**Interfaces:**
- Consumes: Task 1 `Scroll::glyphs()` accessor, `Contents`.

- [ ] **Step 1: Update the common harness accessor**

In `apps/emet/tests/common/mod.rs`, the helper reads `scroll.glyphs` (a field). Change the field access `.glyphs` to the `.glyphs()` method — the accessor returns `&[Glyph]`, so clone into a `Vec`. In `single_scroll_glyphs` (lines 18–27), change:

```rust
            c.scrolls.into_iter().next().unwrap().glyphs
```

to:

```rust
            c.scrolls.into_iter().next().unwrap().glyphs().to_vec()
```

- [ ] **Step 2: Sweep every remaining `Scroll {` literal and `.glyphs` field access in emet tests/src**

Find them:

Run: `rg -n 'Scroll \{|\.glyphs' apps/emet/src apps/emet/tests`

For each **struct literal** `Scroll { name: …, glyphs: <v> }`, rewrite to `Scroll { name: …, policy: None, contents: Contents::Glyphs(<v>) }` and import `Contents` (`use emet::ir::Contents;` in tests, or `scroll_format::Contents` in src). For each **field read** `s.glyphs` where `s: Scroll`, change to `s.glyphs()` (returns `&[Glyph]`; add `.to_vec()` if an owned `Vec` was expected, or adjust the assertion to compare against a slice). Concretely for `apps/emet/tests/scrolls.rs`, the `Scroll { name: "web".into(), glyphs: vec![…] }` assertions at lines 29–35 and 39–42 become:

```rust
        Scroll {
            name: "web".into(),
            policy: None,
            contents: Contents::Glyphs(vec![
                Glyph::AptPackage { name: "nginx".into() },
                Glyph::SystemdService { unit: "nginx.service".into() },
            ]),
        }
```

and the `ss[0].glyphs == ss[1].glyphs` comparison at line 57 becomes `ss[0].glyphs() == ss[1].glyphs()`. Add `use emet::ir::Contents;` to the `scrolls.rs` imports (line 8).

For `apps/emet/tests/lichess.rs`, `quadlet.rs`, `modules.rs`, `library_search_path.rs`: apply the same two mechanical rewrites (`.glyphs` → `.glyphs()`; any `Scroll { … glyphs: … }` literal → `contents: Contents::Glyphs(…)`). Most of these read `.glyphs` on a compiled scroll; those become `.glyphs()`.

For `apps/emet/src/main.rs` (2 `.glyphs` uses — the `--text` renderer) and `apps/emet/src/lib.rs` (`analyze`, 1 use): `analyze` iterates a scroll's glyphs to detect key conflicts *per scroll*. Under the recursive model, a conflict is per **leaf unit** (ADR 0031 §1: each leaf is one conflict scope). Change `analyze`'s glyph source from `scroll.glyphs` to iterating `scroll.leaf_units()` and checking key conflicts *within each leaf's glyphs*. In `apps/emet/src/lib.rs`, locate the analyze loop (`grep -n 'glyphs\|analyze' apps/emet/src/lib.rs`) and replace the per-scroll glyph iteration with:

```rust
        for unit in scroll.leaf_units() {
            let mut seen: std::collections::BTreeMap<String, &Glyph> = std::collections::BTreeMap::new();
            for glyph in unit.glyphs {
                let key = glyph.key();
                if let Some(prev) = seen.get(&key) {
                    if *prev != glyph {
                        return Err(/* the existing conflict Error for `key`, keep its exact construction */);
                    }
                } else {
                    seen.insert(key, glyph);
                }
            }
        }
```

Preserve the exact `Error`/`Phase::Analyze` construction the current code uses for the conflict message (the test `conflicting_glyph_keys_within_one_scroll_is_analyze_error` asserts the message contains `file:/etc/motd`). Read the current analyze body first and keep its error shape verbatim; only the *iteration source* changes (from `scroll.glyphs` to per-leaf-unit).

For `apps/emet/src/main.rs`'s `--text` renderer, change `scroll.glyphs` to `scroll.all_glyphs()` (flatten all leaves for the flat text view) or render the tree; the minimal change is `scroll.all_glyphs()` returning `Vec<&Glyph>` — adjust the loop to iterate references.

- [ ] **Step 3: Add a per-leaf conflict-scope test**

Append to `apps/emet/tests/recursive_scroll.rs`:

```rust
#[test]
fn same_glyph_key_in_two_sibling_leaves_does_not_conflict() {
    let src = r#"
main =
  [ scroll { name = "host", groups =
      [ scroll { name = "a", glyphs = [ file { path = "/etc/x", contents = "1", mode = "0644" } ] }
      , scroll { name = "b", glyphs = [ file { path = "/etc/x", contents = "2", mode = "0644" } ] }
      ] }
  ]
"#;
    let ss = scrolls(src);
    assert_eq!(ss.len(), 1);
}

#[test]
fn conflicting_keys_within_one_leaf_is_analyze_error() {
    let src = r#"
main =
  [ scroll { name = "host", groups =
      [ scroll { name = "a", glyphs =
          [ file { path = "/etc/x", contents = "1", mode = "0644" }
          , file { path = "/etc/x", contents = "2", mode = "0644" }
          ] }
      ] }
  ]
"#;
    let e = err(src);
    assert_eq!(e.phase, Phase::Analyze);
    assert!(e.msg.contains("file:/etc/x"), "got: {}", e.msg);
}
```

- [ ] **Step 4: Run the whole emet suite**

Run: `cargo test -p emet`
Expected: PASS across all suites — the harness, `scrolls`, `recursive_scroll`, `lichess`, `quadlet`, `modules`, `library_search_path`, and the in-crate `src` tests.

- [ ] **Step 5: Commit**

```bash
git add apps/emet/tests/common/mod.rs apps/emet/tests/scrolls.rs apps/emet/tests/lichess.rs apps/emet/tests/quadlet.rs apps/emet/tests/modules.rs apps/emet/tests/library_search_path.rs apps/emet/tests/recursive_scroll.rs apps/emet/src/main.rs apps/emet/src/lib.rs
git commit -m "test(emet): migrate suites and analyze to recursive Scroll

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** document that `analyze` conflict scope is now the leaf unit (per ADR 0031 §1), and that sibling leaves may share a glyph key; that `--text` flattens all leaves for its summary view.

---

## Task 9: emet — verify every repo `.emet` program still compiles

**Files:**
- Test: a new integration test that compiles the repo's example programs; and manual `cargo run` checks. No source changes expected (this is the acceptance gate for "existing programs keep working").

**Interfaces:**
- Consumes: the whole Task 1–8 stack.

- [ ] **Step 1: Compile-check every repo `.emet` program manually**

Run each and confirm exit 0 (compile succeeds; `--text` prints a plan):

```bash
for f in \
  apps/emet/examples/oneliner.emet \
  apps/emet/examples/numbered-nodes.emet \
  apps/emet/examples/roles.emet \
  apps/emet/examples/single-host.emet \
  apps/emet/examples/basic.emet \
  apps/emet/examples/fleet.emet \
  apps/emet/examples/heterogeneous-fleet.emet \
  apps/emet/examples/record-hosts.emet \
  apps/emet/examples/config-file.emet \
  apps/emet/examples/app-helper.emet \
  apps/emet/examples/optional-port.emet \
  apps/emet/examples/roles.emet \
  apps/fleet/smoke.emet \
  apps/fleet/reload-proof.emet \
  examples/lichess/fleet.emet \
  examples/registry/registry.emet \
  examples/registry/clients.emet \
  examples/website/website.emet \
  examples/website/builder.emet ; do
    echo "=== $f ==="; cargo run -q -p emet -- --text "$f" >/dev/null || echo "FAILED: $f"
done
```

Expected: no `FAILED:` line. (`examples/registry/clients.emet`, `examples/website/builder.emet`, and `Lichess.emet`/`Fleet.emet` are library modules imported by an entry; compile the *entry* files listed above, which resolve the imports over `emet.json`. If a bare library file errors with "no main," that is expected — only entry modules have `main`.)

- [ ] **Step 2: Add a golden compile-list test**

Create `apps/emet/tests/repo_examples_compile.rs`. It compiles each entry program through `compile_file` (the multi-module path) and asserts success. Model it on `apps/emet/tests/examples.rs` (which already compiles the crate's own examples — read it first for the exact helper it uses, likely `compile_file`). Add the repo-level entry programs:

```rust
use std::path::Path;

fn compiles(entry: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join(entry);
    match emet::compile_file(&path) {
        Ok(_) => {}
        Err(e) => panic!("{entry} failed: {:?}: {}", e.phase, e.msg),
    }
}

#[test]
fn fleet_smoke_programs_compile() {
    compiles("apps/fleet/smoke.emet");
    compiles("apps/fleet/reload-proof.emet");
}

#[test]
fn example_fleets_compile() {
    compiles("examples/lichess/fleet.emet");
    compiles("examples/registry/registry.emet");
    compiles("examples/website/website.emet");
}
```

Verify `emet::compile_file` is the public entry (per `apps/emet/CLAUDE.md`, `compile_file(entry)` runs the resolve stage). If the signature differs (e.g. takes `&str` or returns a different type), match `apps/emet/tests/examples.rs`'s usage exactly.

- [ ] **Step 3: Run**

Run: `cargo test -p emet --test repo_examples_compile`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/emet/tests/repo_examples_compile.rs
git commit -m "test(emet): guard that repo example fleets still compile on v3

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** none (test-only guard). Optionally note in the docs that all shipped example fleets are compile-gated.

---

## Task 10: golemd — flatten leaf glyphs for the diff and select the recursive host scroll

**Files:**
- Modify: `apps/golemd/src/reconcile.rs`
- Modify: `apps/golemd/src/foreman.rs` (only the `.glyphs` field reads at `apply_manifest` logging and `empty_scroll`; and `reconcile.rs` test helpers)
- Test: `apps/golemd/src/reconcile.rs` in-crate tests (update the `scroll` helper), plus a new flatten test.

**Interfaces:**
- Consumes: Task 1 `Scroll::all_glyphs()`, `Contents`.
- Produces: `plan(prior, desired)` still returns `Vec<GlyphOp>` diffed per glyph, now over **every leaf glyph** of the (possibly nested) `desired` scroll, in source order.

- [ ] **Step 1: Write the failing test**

In `apps/golemd/src/reconcile.rs`, the test module's `scroll` helper (lines 75–77) builds `Scroll { name, glyphs }`. Add a nested-scroll flatten test and update the helper. First add (inside `mod tests`):

```rust
    fn nested(children: Vec<(&str, Vec<Glyph>)>) -> Scroll {
        Scroll {
            name: "host".into(),
            policy: None,
            contents: scroll_format::Contents::Groups(
                children
                    .into_iter()
                    .map(|(name, glyphs)| Scroll {
                        name: name.into(),
                        policy: None,
                        contents: scroll_format::Contents::Glyphs(glyphs),
                    })
                    .collect(),
            ),
        }
    }

    #[test]
    fn plan_flattens_nested_leaves_in_source_order() {
        let desired = nested(vec![("a", vec![apt("one")]), ("b", vec![apt("two")])]);
        let ops = plan(&[], &desired);
        assert_eq!(
            ops,
            vec![
                GlyphOp::Install { cid: glyph_content_id(&apt("one")), glyph: apt("one") },
                GlyphOp::Install { cid: glyph_content_id(&apt("two")), glyph: apt("two") },
            ]
        );
    }
```

- [ ] **Step 2: Update the `scroll` test helper to the new shape**

Change the existing `scroll` helper (lines 75–77) to:

```rust
    fn scroll(glyphs: Vec<Glyph>) -> Scroll {
        Scroll { name: "h1".into(), policy: None, contents: scroll_format::Contents::Glyphs(glyphs) }
    }
```

- [ ] **Step 3: Run to verify failure**

Run: `cargo test -p golemd --lib plan_flattens_nested_leaves_in_source_order`
Expected: compile error first (the `plan` function still reads `desired.glyphs`, a field that no longer exists), then a logic failure once it compiles against the accessor.

- [ ] **Step 4: Flatten in `plan`**

In `apps/golemd/src/reconcile.rs`, change the desired-glyph iteration (line 27) from `for glyph in &desired.glyphs {` to:

```rust
    for glyph in desired.all_glyphs() {
```

`all_glyphs()` returns `Vec<&Glyph>`, so `glyph` is now `&&Glyph` inside the loop only if bound by ref; bind by value: `for glyph in desired.all_glyphs()` yields `&Glyph`. The body uses `glyph.key()`, `glyph_content_id(glyph)`, and `glyph.clone()` — all valid on `&Glyph`. No other change to `plan` (the removes pass reads `prior`, untouched here).

- [ ] **Step 5: Fix `foreman.rs` `.glyphs` reads**

In `apps/golemd/src/foreman.rs`, `apply_manifest`'s log line reads `selected.scroll.glyphs.len()` (line 97). Change to `selected.scroll.all_glyphs().len()`. `empty_scroll` (line 614) builds `Scroll { name, glyphs: vec![] }` — change to:

```rust
fn empty_scroll(host: &str) -> Scroll {
    Scroll { name: host.to_string(), policy: None, contents: scroll_format::Contents::Glyphs(vec![]) }
}
```

Sweep any other `Scroll {` literal or `.glyphs` field read in `foreman.rs` (the in-crate test module builds scrolls too):

Run: `rg -n 'Scroll \{|\.glyphs' apps/golemd/src/foreman.rs`

Rewrite each test-helper `Scroll { name, glyphs: v }` to `Scroll { name, policy: None, contents: scroll_format::Contents::Glyphs(v) }` and each `.glyphs` read to `.glyphs()` (leaf) or `.all_glyphs()` (flatten), matching the assertion.

- [ ] **Step 6: Run the golemd crate tests**

Run: `cargo test -p golemd --lib`
Expected: PASS (the new flatten test plus all existing in-crate tests). Integration tests under `apps/golemd/tests/` may still fail to compile — fixed in Task 12.

- [ ] **Step 7: Commit**

```bash
git add apps/golemd/src/reconcile.rs apps/golemd/src/foreman.rs
git commit -m "feat(golemd): diff over flattened leaf glyphs of the recursive Scroll

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** document that the diff is still per-glyph and grouping is not a diff input (ADR 0031 §4); `all_glyphs()` flattens in source order; `empty_scroll` is a leaf with no glyphs.

---

## Task 11: golemd — carry `unit_path` through the WAL step and its sqlite column

**Files:**
- Modify: `apps/golemd/src/journal.rs` (`WalStep` gains `unit_path`)
- Modify: `apps/golemd/src/planroom.rs` (`wal_step` table + `append_wal_step` + row read)
- Modify: `apps/golemd/src/foreman.rs` (`enact`/`enact_apply`/`enact_reverse`/`rollback_attempt` thread a `unit_path`, computed per leaf but for Plan 1 carried as the whole-host flattened path)
- Modify: `apps/golemd/src/wal.rs` (`outcome_of` construction — carry the field through where a `WalStep` is built in tests/helpers)
- Test: `apps/golemd/src/planroom.rs` in-crate tests (round-trip a step with a `unit_path`).

**Interfaces:**
- Consumes: Task 1 types.
- Produces: `WalStep.unit_path: Vec<String>`; `PlanRoom::append_wal_step` gains a `unit_path: &[String]` parameter (last positional arg); the `wal_step` table has a `unit_path TEXT NOT NULL DEFAULT '[]'` column holding serde_json. **Plan 2 consumes this parameter** to pass each op's true leaf name-path; Plan 1 passes the host root path (a single-element or flattened path) so nothing is lost.

- [ ] **Step 1: Write the failing test**

In `apps/golemd/src/planroom.rs`'s test module, the WAL round-trip test builds and reads a `WalStep`. Add a test that a `unit_path` survives the round-trip. Locate the WAL test (`grep -n 'wal' apps/golemd/src/planroom.rs`) and add:

```rust
    #[test]
    fn wal_step_round_trips_unit_path() {
        let room = SqlitePlanRoom::open(std::path::Path::new(":memory:")).unwrap();
        let attempt = room.open_attempt(None).unwrap();
        let op = GlyphOp::Install {
            cid: sample_cid(),
            glyph: Glyph::AptPackage { name: "nginx".into() },
        };
        room.append_wal_step(
            attempt.reconcile_id,
            0,
            "apt:nginx",
            WalAction::Apply,
            WalStepState::Intended,
            &op,
            None,
            None,
            &["worker".to_string(), "base".to_string()],
        )
        .unwrap();
        let steps = room.wal_steps_for(attempt.reconcile_id).unwrap();
        assert_eq!(steps[0].unit_path, vec!["worker".to_string(), "base".to_string()]);
    }
```

The in-memory planroom is `SqlitePlanRoom::open(Path::new(":memory:"))` (there is no `open_in_memory`; the existing `planroom.rs` tests use the `:memory:` path). `sample_cid` may be named differently — read the module and reuse its `ContentId` fixture and any `sample()`/`roundtrip()` helper. The load-bearing point: `append_wal_step` gains a final `&[String]` argument, and the read-back `WalStep` exposes `unit_path`.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p golemd --lib wal_step_round_trips_unit_path`
Expected: compile error — `append_wal_step` takes no `unit_path` argument and `WalStep` has no `unit_path` field.

- [ ] **Step 3: Add the field to `WalStep`**

In `apps/golemd/src/journal.rs`, add to the `WalStep` struct (lines 230–241, after `changed`):

```rust
    pub unit_path: Vec<String>,
```

- [ ] **Step 4: Add the column and thread the parameter in `planroom.rs`**

In `apps/golemd/src/planroom.rs`:

(a) In the `wal_step` `CREATE TABLE` (the schema at lines ~152–180), add a column after `changed`:

```sql
    unit_path    TEXT NOT NULL DEFAULT '[]',
```

(b) In the `PlanRoom` trait `append_wal_step` signature (lines ~54–66), add a final parameter `unit_path: &[String]`.

(c) In `SqlitePlanRoom::append_wal_step` (the impl at lines ~273–317), serialize `unit_path` with `serde_json::to_string(unit_path)?` and bind it in the `INSERT` (add the column and a `?` placeholder). Include it in the `WalStep` the method returns.

(d) In the row-reading query/mapping (`wal_steps` / `wal_steps_for`, lines ~329–337 and the shared row mapper), select the `unit_path` column and `serde_json::from_str` it into the `WalStep.unit_path` field. If a column is `NULL`/absent on an old row, default to `vec![]` (the `DEFAULT '[]'` handles fresh dbs; the disposable fleets re-init per ADR 0031 §5, so no migration of old rows is required — but a defensive `.unwrap_or_default()` on the parse keeps a stale db from panicking).

(e) If there is an in-memory `PlanRoom` implementation (the tests mention `sqlite_and_memory_wal_behave_the_same`), add the same `unit_path` carry to its `append_wal_step` and stored `WalStep`.

- [ ] **Step 5: Thread `unit_path` through the foreman enact spine**

In `apps/golemd/src/foreman.rs`, every `self.planroom.append_wal_step(…)` call (in `enact_apply` lines 206/218/231, `enact_reverse` lines 257/269/282, and `rollback_attempt` line 442) gains a trailing `unit_path` argument. For Plan 1, the value is the **host root path** — a single-element `vec![self.host.clone()]` — carried uniformly, since Plan 1 does not yet split enact per leaf unit. Add a `unit_path: &[String]` parameter to `enact_apply` and `enact_reverse`, plumb it from `enact`:

In `enact` (line 158), before the op loop, bind `let unit_path = [self.host.clone()];` and pass `&unit_path` into each `enact_apply`/`enact_reverse` call. In `rollback_attempt` (line 442), pass the reversing step's own recorded path: `&target.unit_path`.

Concretely, `enact_apply`'s signature becomes:

```rust
    fn enact_apply(
        &self,
        reconcile_id: u64,
        ord: u64,
        op: &GlyphOp,
        glyph: &Glyph,
        cid: ContentId,
        intended_inverse: Option<&Inverse>,
        unit_path: &[String],
    ) -> Result<()> {
```

and each `append_wal_step(…)` inside it gains a trailing `unit_path` argument. Same shape for `enact_reverse` (add `unit_path: &[String]`, pass through).

- [ ] **Step 6: Carry the field wherever a `WalStep` is constructed in `wal.rs`/tests**

Run: `rg -n 'WalStep \{' apps/golemd/src apps/golemd/tests`

Any literal `WalStep { … }` (test fixtures, `wal.rs` helpers) gains `unit_path: vec![]` (or a representative path). `outcome_of` in `wal.rs` reads a `WalStep` to build an `Outcome` — `Outcome` has no `unit_path`, so `outcome_of` is unchanged; only literal constructions of `WalStep` need the new field.

- [ ] **Step 7: Run the golemd crate tests**

Run: `cargo test -p golemd --lib`
Expected: PASS including `wal_step_round_trips_unit_path` and `sqlite_and_memory_wal_behave_the_same`.

- [ ] **Step 8: Commit**

```bash
git add apps/golemd/src/journal.rs apps/golemd/src/planroom.rs apps/golemd/src/foreman.rs apps/golemd/src/wal.rs
git commit -m "feat(golemd): carry a unit_path on every WAL step

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** document that `unit_path` is additive and *carried, not consulted* by recovery in Plan 1 (ADR 0031 §6); the `DEFAULT '[]'` column; that Plan 2 replaces the host-root placeholder with the true leaf name-path; and that the bracketing invariant and `step_ord`+`action` grouping are unchanged.

---

## Task 12: golemd — fix integration tests and confirm the whole crate is green

**Files:**
- Modify: `apps/golemd/tests/config_propagation.rs`, `apps/golemd/tests/revisions_projection.rs`, `apps/golemd/tests/wal_recovery.rs`, `apps/golemd/tests/wal_replace_and_fold.rs`
- Test: all golemd tests.

**Interfaces:**
- Consumes: Tasks 10–11.

- [ ] **Step 1: Sweep the integration tests for the old shape**

Run: `rg -n 'Scroll \{|\.glyphs|append_wal_step|WalStep \{' apps/golemd/tests`

For each `Scroll { name, glyphs: v }` → `Scroll { name, policy: None, contents: scroll_format::Contents::Glyphs(v) }` (import `scroll_format::Contents`). For each `.glyphs` field read → `.glyphs()` or `.all_glyphs()`. For each direct `append_wal_step(…)` call → add the trailing `unit_path` arg (`&[]` or a representative `&["host".to_string()]`). For each `WalStep { … }` literal → add `unit_path: vec![]`.

- [ ] **Step 2: Run each integration test**

Run: `cargo test -p golemd`
Expected: PASS across `--lib` and all four `tests/*.rs` suites.

- [ ] **Step 3: Commit**

```bash
git add apps/golemd/tests/config_propagation.rs apps/golemd/tests/revisions_projection.rs apps/golemd/tests/wal_recovery.rs apps/golemd/tests/wal_replace_and_fold.rs
git commit -m "test(golemd): migrate integration tests to recursive Scroll and unit_path

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** none (test migration).

---

## Task 13: fleet — keep `_render_revision` working; fix the `state` view's `scroll.glyphs` access

**Files:**
- Modify: `apps/fleet/cli.py`
- Test: manual (there are no CLI tests today; a mocked test is Plan 2's job when the report shape changes).

**Interfaces:**
- Consumes: the golemd `/state` JSON shape, whose `scroll` is now a recursive `Scroll` (its `contents` is `{"Glyphs": [...]}` or `{"Groups": [...]}`; there is no top-level `glyphs` key). The revision JSON returned by `apply` is **unchanged in Plan 1** (still `Revision` with `outcomes`), so `_render_revision`/`_glyph_desc`/`_op_parts` need no change.

- [ ] **Step 1: Identify the `scroll.glyphs` access**

The `status` command (cli.py lines ~297–341) reads `view.get("scroll")` then `scroll.get("glyphs")` to count glyphs. Under the recursive `Scroll`, the JSON is `{"name": …, "policy": …, "contents": {"Glyphs": [...]}}` (or `{"Groups": [...]}`). The glyph count must recurse.

- [ ] **Step 2: Add a recursive glyph-count helper and use it**

Add near the other `_` helpers in `apps/fleet/cli.py`:

```python
def _count_glyphs(scroll: object) -> int:
    if not isinstance(scroll, dict):
        return 0
    contents = scroll.get("contents")
    if not isinstance(contents, dict):
        return 0
    if "Glyphs" in contents:
        glyphs = contents.get("Glyphs")
        return len(glyphs) if isinstance(glyphs, list) else 0
    if "Groups" in contents:
        groups = contents.get("Groups")
        if isinstance(groups, list):
            return sum(_count_glyphs(child) for child in groups)
    return 0
```

In the `status` command, replace the block that computes `glyph_count` from `scroll.get("glyphs")` with:

```python
            scroll = view.get("scroll")
            glyph_count = str(_count_glyphs(scroll)) if scroll is not None else "—"
```

- [ ] **Step 3: Manual verification**

Since there is no golemd running in CI for the fleet suite, verify by constructing the JSON shape in a Python REPL:

```bash
python3 -c "
import sys; sys.path.insert(0, 'apps')
from fleet.cli import _count_glyphs
flat = {'name':'h','contents':{'Glyphs':[{'AptPackage':{'name':'nginx'}}]}}
nested = {'name':'h','contents':{'Groups':[flat, flat]}}
assert _count_glyphs(flat) == 1, _count_glyphs(flat)
assert _count_glyphs(nested) == 2, _count_glyphs(nested)
print('ok')
"
```

Expected: `ok`.

- [ ] **Step 4: Run the existing fleet tests**

Run: from repo root, `PYTHONPATH=apps python -m pytest apps/fleet/tests -q` (or the project's configured test runner; the fleet tests are `unittest`-style, so `python -m unittest discover apps/fleet/tests` also works).
Expected: PASS (these tests are about ports/resume, not the CLI render path, so they are unaffected — this run just confirms no import break).

- [ ] **Step 5: Commit**

```bash
git add apps/fleet/cli.py
git commit -m "fix(fleet): count glyphs recursively in the state view

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

**Doc backlog:** note that `_count_glyphs` recurses `contents.Groups`; the revision render path is untouched in Plan 1 (the report shape changes only in Plan 2).

---

## Task 14: whole-workspace green + acceptance gate

**Files:**
- Test: the whole workspace.

**Interfaces:**
- Consumes: all prior tasks.

- [ ] **Step 1: Full workspace test**

Run: `cargo test --workspace`
Expected: PASS across `scroll-format`, `emet`, `emet-lsp`, `golemd`, `golemctl`. If `emet-lsp` or `golemctl` reference `Scroll { … glyphs }` or `.glyphs`, sweep them the same way (`rg -n 'Scroll \{|\.glyphs' apps/emet-lsp apps/golemctl`) and fix; add those files to a follow-up commit.

- [ ] **Step 2: Build the release binaries the quickstart names**

Run: `cargo build --release -p golemd -p golemctl -p emet`
Expected: builds clean.

- [ ] **Step 3: Acceptance — a flat program and a nested program both apply through the fake reconciler**

Compile and inspect both shapes:

```bash
cargo run -q -p emet -- --text apps/fleet/smoke.emet
```

Expected: prints a plan for the flat leaf scroll (unchanged behavior).

Create a scratch nested program and confirm it compiles and its leaf units are visible in `--json`:

```bash
cat > /tmp/nested.emet <<'EOF'
main =
  [ scroll { name = "worker", groups =
      [ scroll { name = "base", glyphs = [ aptPackage { name = "htop" } ] }
      , scroll { name = "app", policy = keep, glyphs = [ aptPackage { name = "nginx" } ] }
      ] }
  ]
EOF
cargo run -q -p emet -- --json /tmp/nested.emet
```

Expected: the JSON shows a scroll named `worker` whose `contents.Groups` holds two leaves, one carrying `policy` with `on_exhaust: "Keep"`.

- [ ] **Step 4: Commit any final sweep**

```bash
git add <any emet-lsp/golemctl files touched in Step 1>
git commit -m "chore: finish recursive Scroll sweep across the workspace

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

(Skip the commit if Step 1 needed no changes.)

**Doc backlog:** update `QUICKSTART.md`'s "The model" and the `scroll { name, glyphs }` example to mention `groups` and per-scroll `policy` (grouping + failure isolation); note the `format_version` is now 3. Update `apps/emet/CLAUDE.md`'s "The four primitives and `Scroll`" section to describe the recursive `Scroll`, `Contents`, and the `policy`/`rollback`/`keep`/`retry` surface. These are documenter-owned.

---

## Self-Review

**1. Spec coverage (ADR 0031):**

- §1 recursive strict tree (`Scroll { name, policy, contents }`, `Contents::Glyphs|Groups`) → Task 1. ✓
- §1 flat scroll stays a leaf → Task 5 (`flat_scroll_still_lowers_to_a_leaf`), Task 9 (repo programs). ✓
- §2 leaf is the unit; `leaf_units`/policy chain helper → Task 1 (`leaf_units`, `policy_chain`). Per-unit *enact* is Plan 2 (correctly deferred; Plan 1 flattens). ✓
- §3 `Policy` fields + `OnExhaust`; every field optional; default rollback → Task 1 (types; the *default* is applied by the resolver in Plan 2, the type carries `Option`). ✓ (flagged below: Plan 1 only carries the type; the `rollback`-default resolution is Plan 2.)
- §4 diff stays on leaves / group identity is name-path / vanished-unit removes → Task 10 (flatten diff), Task 11 (`unit_path`). Vanished-unit-removes-under-parent-policy is Plan 2 semantics. ✓
- §5 wire order + `FORMAT_VERSION` 2→3 + golden bytes → Tasks 1–3. ✓
- §6 WAL `unit_path` field/column; tree-shaped `ReconcileReport` → Task 11 (WAL column); the report is Plan 2. ✓
- §7 Emet surface (`scroll` name+policy+glyphs-xor-groups; `rollback`/`keep`/`retry`; first-class types) → Tasks 4–8. ✓

**2. Placeholder scan:** The two deliberately-templated spots are Task 3 Step 4 (paste captured golden bytes — unavoidable; the regeneration harness in Steps 2–3 produces the exact values) and Task 8 Step 2's analyze-error construction ("keep its exact construction" — the implementer must read the current `lib.rs` analyze error and preserve it verbatim; the plan cannot show bytes it did not read, but names the exact test the wording must satisfy). No `TODO`/`handle edge cases`/`add validation` placeholders remain. Every code step shows code.

**3. Type consistency:** `Scroll { name, policy, contents }`, `Contents::Glyphs|Groups`, `Policy` (7 optional fields, exact order), `OnExhaust::Rollback|Keep`, `LeafUnit { path, glyphs, policy_chain }`, `Scroll::glyphs()`/`is_leaf()`/`all_glyphs()`/`leaf_units()` are used identically in Tasks 1, 3, 5, 6, 7, 8, 10, 11. AST `ContentsExpr::Glyphs|Groups`, `OnExhaustTag::Rollback|Keep`, `Expr::PolicyExhaust`/`Expr::PolicyRetry` are consistent across Tasks 4–7. `append_wal_step`'s new trailing `unit_path: &[String]` and `WalStep.unit_path: Vec<String>` are consistent across Tasks 11–12 and are the exact signatures Plan 2 consumes. Retry record camelCase field names (`maxAttempts`, `baseDelayMs`, `backoffMultiplier`, `maxDelayMs`, `jitterFraction`, `maxElapsedMs`, `onExhaust`) match between parser/infer/eval (Tasks 5–7). One naming check: `apply_manifest`/`plan`/`enact_apply`/`enact_reverse`/`rollback_attempt` signatures match the studied code.

**Flagged interpretation (wire-visible — do not silently decide):** see the plan-level report; the retry-record surface field names and the fact that `onExhaust`'s value types as `Policy` (not a distinct `OnExhaust` type) are surface-only choices (not wire), but I chose them explicitly.
