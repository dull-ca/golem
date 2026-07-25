# Quickstart

> **The model.** You author a fleet in **Emet** — a program that evaluates to
> one **scroll** per host. A scroll is a recursive tree: each level holds either
> glyphs (a leaf) or named sub-scrolls (a branch), never both. Glyphs come over
> four kinds (`aptPackage`, `systemdService`, `file`, `lineInFile`); branches
> group leaves into named units. `emetc` compiles it to a binary,
> content-addressed **manifest** (`format_version` 3). A per-host `golemd`
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

The policy is carried on the wire today; the per-unit enact that reads it lands
with ADR 0029's implementation (see `docs/adr/0031`). Grouping and policy change
no glyph's content id, so regrouping or renaming a unit re-enacts nothing.

A worked, multi-host, multi-module example is in `examples/lichess/` — a shared
`Lichess` abstraction library and `Fleet` fact table, imported by the `fleet.emet`
entry module. `examples/lichess/run.sh` drives the whole flow end to end.

## Apply and inspect

`golemctl apply` takes a `.emet` source (it runs `emetc build` for you) or a
prebuilt `.manifest`, and POSTs the manifest bytes to the node:

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
- **No signing.** Anyone who can reach the agent's port can apply a manifest.
  Trust is its own concern.
- **No multi-node coordination.** One agent per host; each enacts only its own
  scroll from the shared manifest.
- **Four glyph kinds only.** Richer shapes (workloads, services, ingress) are
  Emet library abstractions that compile down to the four glyphs — never new
  golemd resource kinds.
