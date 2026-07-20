# Quickstart

> **The model.** You author a fleet in **Emet** — a program that evaluates to
> one **scroll** per host, each a list of glyphs over four kinds (`aptPackage`,
> `systemdService`, `file`, `lineInFile`). `emetc` compiles it to a binary,
> content-addressed **manifest**. A per-host `golemd` ingests the manifest,
> selects its own scroll, diffs it by content id, and enacts the difference
> through reversible reconcilers, journalling what it did so every change can be
> undone. By default `golemd` runs the **fake** reconciler, which records intent
> without touching the host — safe to run anywhere.

## Build

```bash
cargo build --release -p golemd -p golemctl -p emet
```

`emetc` is the `emet` crate's binary; put it on `PATH` (or `cargo install
--path emet/crates/emet`) so `golemctl apply` can invoke it on a `.emet` source.

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
returns the revision it recorded.

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
