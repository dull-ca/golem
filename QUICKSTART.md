# Quickstart

> **The model.** You author a fleet in **Emet** — a program that evaluates to
> one **scroll** per host. A scroll is a recursive tree: each level holds either
> glyphs (a leaf) or named sub-scrolls (a branch), never both. Glyphs come over
> four kinds (`aptPackage`, `systemdService`, `file`, `lineInFile`); branches
> group leaves into named units. `emetc` compiles it to a binary,
> content-addressed **manifest** (`format_version` 4). A per-host `golemd`
> ingests the manifest, selects its own scroll, diffs it by content id, and
> enacts the difference through reversible reconcilers, journalling what it did
> so every change can be undone. By default `golemd` runs the **fake**
> reconciler, which records intent without touching the host — safe to run
> anywhere.

## Build

```bash
cargo build --release -p golemd -p golemctl -p emet
```

`emetc` is the `emet` crate's binary; put it on `PATH` (or `cargo install
--path apps/emet`) so `golemctl apply` can invoke it on a `.emet` source.

## Run the agent

```bash
./target/release/golemd --host dev-01 \
  --state-dir /tmp/golem-state \
  --listen 127.0.0.1:7474
```

`--host` names which scroll this node enacts. State lives in
`/tmp/golem-state/planroom.db` (SQLite, WAL); removing that directory resets the
node. Add `--reconciler host` to enact for real (apt/systemd/file); the default
is `--reconciler fake`.

`--config golemd.toml` points at a config file whose `[retry]` block sets
fleet-wide retry defaults (backoff, jitter, attempt and wall-time limits); a
per-scroll `policy` overrides them. Absent, built-in defaults apply.

## Author a fleet in Emet

A program evaluates to `main : List Scroll`. The smallest useful one:

```elm
module Main exposing (..)

web : Scroll
web =
  scroll
    { name = "dev-01"
    , glyphs =
        [ aptPackage { name = "nginx" }
        , systemdService { unit = "nginx.service" }
        ]
    }

main : List Scroll
main = [ web ]
```

That `scroll { name, glyphs }` is a **leaf** — one unit of glyphs. To run many
distinct units on one host, nest them with `groups`; each level is `glyphs` xor
`groups`, never both:

```elm
worker : Scroll
worker =
  scroll
    { name = "worker-01"
    , groups =
        [ scroll { name = "engine", glyphs = [ aptPackage { name = "stockfish" } ] }
        , scroll { name = "base",   glyphs = [ aptPackage { name = "curl" } ] }
        ]
    }
```

A leaf is the **failure-isolation unit**: one unit failing doesn't roll back its
siblings. A scroll may carry an optional `policy` — `rollback` (the default),
`keep`, or `retry { maxAttempts = 3, onExhaust = keep, … }` — that governs how a
unit's enact retries and what it does when the budget is exhausted:

```elm
scroll { name = "engine", policy = keep, glyphs = [ … ] }
```

The policy is carried on the wire and enforced per unit: golemd reads each unit's
resolved policy and applies best-effort enact, retries, and `on_exhaust` to that
unit alone (ADR 0029, ADR 0031). Grouping and policy change no glyph's content id,
so regrouping or renaming a unit re-enacts nothing.

A worked, multi-host, multi-module example is in `examples/lichess/` — a shared
`Lichess` abstraction library and `Fleet` fact table, imported by the `fleet.emet`
entry module. `examples/lichess/run.sh` drives the whole flow end to end.

## Plan, apply, inspect

`golemctl plan` shows what an apply *would* do without doing it — the node
diffs the manifest against its journal and returns the ordered operations,
collapsed one line per action, with the coalesced reload step last (ADR 0036):

```bash
./target/release/golemctl plan examples/lichess/fleet.emet http://127.0.0.1:7474
```

Add `--detail` for per-glyph content ids, `--json` for the raw response. A
plan never writes anything and is safe to run while an apply is in flight.

`golemctl apply` takes the same `.emet` source (it runs `emetc build` for you)
or a prebuilt `.manifest`, and POSTs the manifest bytes to the node:

```bash
./target/release/golemctl apply examples/lichess/fleet.emet http://127.0.0.1:7474
```

The node selects the scroll named for its `--host`, reconciles toward it, and
returns a per-unit report of what settled and what failed. A partial or
rolled-back reconcile is still HTTP 200 with its failures in-band; a
transport/daemon error is non-2xx with an actionable message.

```bash
./target/release/golemctl state   http://127.0.0.1:7474   # current applied scroll + content id
./target/release/golemctl history http://127.0.0.1:7474   # the revision journal
./target/release/golemctl show    http://127.0.0.1:7474 3 # one revision by id
```

## Apply to a whole fleet

A manifest already carries every host's scroll, so applying it everywhere is one
command over a declared set of endpoints. That set is a TOML **inventory**
mapping a host name to the base URL of the golemd that serves it:

```toml
# fleet.toml
[hosts]
scaly = "http://127.0.0.1:8807"

[hosts.manta]
url = "http://127.0.0.1:8842"
```

Both value shapes carry the same one fact; the table form is where per-host
connection options will land later. golemctl looks for the file at
`--inventory`, then `$GOLEMCTL_INVENTORY`, then `./fleet.toml`, then
`./.fleet/inventory.toml` (the file the VM harness writes).

```bash
./target/release/golemctl fleet plan   examples/lichess/fleet.emet
./target/release/golemctl fleet apply  examples/lichess/fleet.emet
./target/release/golemctl fleet status
```

`fleet apply` compiles once, fires every host's `POST /manifest` concurrently,
and follows them all — on a terminal, one live tree with a branch per host over
that host's usual unit tree; otherwise host-prefixed plain lines, and with
`--json` a single `{"hosts": {…}}` object on stdout. `fleet plan` is the same
fan-out over the dry run, each host's diff indented under its own heading — the
host name and its address — and so is the summary `fleet apply` closes with.
`fleet status` gives one marked line per host, columns aligned: `✓` with its
latest revision and applied content id, `·` where nothing has been applied yet,
`✗` with the reason where the daemon could not be reached. `--hosts a,b` narrows
any of them to a subset; an unknown name is an error naming the ones the
inventory declares.

Each host is isolated: a transport failure, a 409 from a daemon already
reconciling, or a rolled-back unit stops that host alone, and every other host
still runs to its terminal phase and is reported.

**Absence is silence.** A host in the inventory that the manifest names no
scroll for is *skipped* — never POSTed to, reported as skipped, and not counted
against the exit code. A daemon resolves a missing scroll to the empty one, so
without this rule a partial manifest would decommission every host it failed to
mention. Decommissioning a host is therefore an explicitly authored empty scroll
for it, not its removal from the program. (Single-host `golemctl apply` keeps
its meaning: naming one daemon is an explicit order.)

Exit codes: `fleet apply` exits 0 only if every host settled or was skipped, 1
otherwise. `fleet plan` exits 0 unless a host errored — a diff is not a failure.
`fleet status` always exits 0; an unreachable host is an observation, not an
assertion.

The local VM harness emits an inventory for its guests, so the same verbs drive
it unchanged (`apps/fleet/README.md`):

```bash
fleet inventory                        # writes .fleet/inventory.toml, found automatically
golemctl fleet apply examples/lichess/fleet.emet
```

## The journal

Every apply is a revision. The first is `init` (empty, written when the node
first boots); each reconcile is a `reconcile` revision carrying the scroll's
content id and the ordered outcomes — each outcome recording the glyph operation
and the `inverse` needed to undo it exactly. golem only ever reverses edits it
recorded, so it never removes a package, line, or file the host already had.

Removing everything is applying a scroll with no glyphs (or omitting this host
from the fleet): the node reverses each recorded outcome and journals the result.

## Manual HTTP, if you prefer

```bash
curl -X POST http://127.0.0.1:7474/manifest \
  --data-binary @examples/lichess/fleet.manifest

curl http://127.0.0.1:7474/state     | jq
curl http://127.0.0.1:7474/revisions | jq
```

## What this isn't (yet)

- **Fake by default.** `--reconciler fake` records intent without touching apt,
  systemd, or the filesystem. `--reconciler host` enacts for real, but the
  end-to-end run against a real Debian box is still being exercised.
- **Trust is the infrastructure's.** Anyone who can reach the agent's port can
  apply a manifest: golemd verifies nothing about a caller and golemctl sends
  no credentials. That is the decision, not a gap — confidentiality and
  authenticity belong to the layer below (unix sockets with file permissions,
  loopback binds, segmentation, ssh tunnels or a mesh VPN), which golem itself
  can provision like anything else (ADR 0040). Binding `--listen` to a routable
  interface publishes root-equivalent control of that host.
- **No daemon-side coordination.** One agent per host, each enacting only its
  own scroll from the shared manifest. `golemctl fleet` fans out from the
  operator's machine, so that machine must reach every daemon; golem-to-golem
  propagation — submit to one, all receive — is designed but unbuilt (ADR 0039).
- **Four glyph kinds only.** Richer shapes (workloads, services, ingress) are
  Emet library abstractions that compile down to the four glyphs — never new
  golemd resource kinds.
