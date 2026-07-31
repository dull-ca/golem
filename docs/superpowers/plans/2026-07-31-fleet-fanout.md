# Fleet fan-out: `golemctl fleet` verbs over a TOML inventory

Implements ADR 0038. Phase 2 (golemd peer gossip) is designed in ADR 0039
and is **not** part of this plan.

## Goal

One command applies (or plans) a manifest against every host in a declared
inventory, concurrently, with per-host result isolation and a fleet-level
live view. The local VM harness emits an inventory for its guests so the
new verbs drive it unchanged.

## Surface

### Inventory (`apps/golemctl/src/inventory.rs`, new)

```toml
# fleet.toml — name → golemd base URL. Names label output; each daemon's
# own --host decides which scroll it enacts.
[hosts]
scaly = "http://127.0.0.1:8807"
manta = "http://127.0.0.1:8842"
```

- `resolve(flag: Option<PathBuf>) -> PathBuf`: `--inventory` > `$GOLEMCTL_INVENTORY` > `./fleet.toml`.
- `load(path) -> Result<Inventory>`: parse; empty `[hosts]` or missing file is
  an actionable error naming the resolution chain.
- `Inventory::select(hosts: Option<&str>) -> Result<Vec<Target>>`: `--hosts a,b`
  filter; unknown name errors listing known names. `Target { name, addr }`.
  Deterministic order: file order (`toml` preserves it via `IndexMap`-like
  ordered table? if not, sort by name — pick one, test it).

### CLI (`main.rs`)

```
golemctl fleet apply  <source> [--inventory P] [--hosts a,b] [--json]
golemctl fleet plan   <source> [--inventory P] [--hosts a,b] [--json] [--detail]
golemctl fleet status          [--inventory P] [--hosts a,b] [--json]
```

`<source>` compiles once via the existing `manifest_bytes` (emetc or
prebuilt manifest), shared across hosts.

**Absence is silence** (Dr. Dub, 2026-07-31): golemctl decodes the
manifest (new `scroll-format` dependency) and `fleet apply` / `fleet
plan` target only inventory hosts whose names the manifest contains. A
host in the inventory but absent from the manifest is *skipped* —
reported (`skipped — no scroll in manifest`, and in the JSON aggregate),
never POSTed to, never affecting the exit code. Decommission requires an
explicitly authored empty scroll. Undecodable manifest bytes error before
any host is contacted.

### `fleet apply` (`apps/golemctl/src/fleet.rs`, new)

- Per target, concurrently: `poll::post_manifest` → loop `poll::get_progress`
  until `Phase::is_terminal`, folding into that host's `ApplyModel`
  (reuse `model.rs` unchanged). A transport error or non-202 (e.g. 409
  reconcile-in-progress) marks that host failed with its message; others
  continue.
- Host outcome: `Settled(report)` / `Unsettled(report)` (partial,
  rolled_back) / `Error(message)`. Exit code 0 iff all `Settled`.
- TTY view: reuse `view::lines(&ApplyModel) -> Vec<Line>` per host; a
  fleet view stacks, per host, a heading line (host name + fast spinner
  while in flight; ✓ settled / ✗ failed-or-error / ↩ rolled back; then
  the host's indented unit-tree lines. Same Live pattern as `apply.rs`
  (model behind Mutex, Notify per fold, `render_loop` on stderr,
  `fit` to terminal height). Keep the per-host `Persistence` log sinks
  (`logs:` line printed once, per host dir).
- Non-TTY / `--json`: plain lines prefixed `[name]`, then per-host
  summaries (`summarize_report` prefixed with the host name); `--json`
  prints one final `{"hosts": {name: {"outcome": …, "report": … }
  | {"error": …}}}` object on stdout.

### `fleet plan`

- Concurrently POST each target's `/plan` (factor the HTTP call out of
  `plan::run` so fan-out reuses it); gather all before rendering.
- Render: per host in inventory order, a heading then the existing
  `plan::render` output; `--json` emits `{"hosts": {name: response |
  {"error": …}}}`. Exit 0 unless any host errored (transport), matching
  single-host plan's contract (plan exits 0 whether or not changes
  exist).

### `fleet status`

- Concurrently GET `/status` and `/state`; one line per host:
  `name  addr  host=<daemon host>  revision=<n>  content_id=<prefix>` or
  `name  addr  unreachable: <err>`. `--json` aggregates. Exit 0 even with
  unreachable hosts (status is an observation, not an assertion).

### VM harness (`apps/fleet`)

- New `fleet inventory [--hosts a,b] [--output PATH]` subcommand: renders
  the `[hosts]` table from `state.json` records
  (`http://127.0.0.1:<golemd_port>`), writes `.fleet/inventory.toml` by
  default, prints the path. Pure rendering helper unit-tested in
  `apps/fleet/tests/test_inventory.py`.

## Dependencies

- golemctl gains `toml = { workspace = true }`; dev-dependency
  `golemd = { path = "../golemd" }` for the integration test.

## Tests

1. `inventory.rs` unit tests: resolution order, parse errors, `--hosts`
   filtering incl. unknown-name error, ordering.
2. `fleet.rs` unit tests: exit-code aggregation over host outcomes;
   plain-line prefixing; JSON aggregate shape.
3. Integration `apps/golemctl/tests/fleet_fanout.rs`: spin three in-process
   golemd routers (Foreman + FakeReconciler + tempdir SqlitePlanRoom,
   `axum::serve` on ephemeral ports — mirror `apps/golemd/tests/*`), write
   an inventory, build a small manifest via `scroll_format` (as golemd's
   tests do), then:
   - fleet apply (plain path) → all three daemons report settled, each
     `/state` shows the expected content id, exit-aggregate is success.
   - a manifest naming only two of the three → the third is skipped and
     untouched (`/state` unchanged), aggregate is still success.
   - one daemon stopped → its host reports the transport error, the other
     two still settle, aggregate is failure.
   - fleet plan → three reports, each naming its host, nothing journaled.
   - fleet status → three lines / JSON entries.
4. Python: inventory rendering test.

## Steps

1. `inventory.rs` + tests.
2. `fleet.rs` orchestration (plain + JSON paths) + `main.rs` wiring + tests.
3. Fleet TUI view (host branches over reused unit trees).
4. Integration test.
5. Harness `fleet inventory` + test.
6. QUICKSTART section ("Apply to a whole fleet") + README pointer in
   `apps/fleet/README.md`.

## Non-goals

- No golemd changes of any kind (ADR 0039 owns those).
- No retry/queue on 409; no reattach-on-conflict.
- No per-host connection options beyond the URL (auth, ssh tunnels). To
  leave room for them, accept both value shapes from day one: `name =
  "url"` and `[hosts.name] url = "…"` — future fields join the table form
  without a format break.
