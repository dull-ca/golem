# Quickstart

> **Scope today.** Golem is a per-node bookkeeper for *package
> declarations*. You submit named declarations (each one a list of
> packages you want present). The node merges them into a canonical
> state — each package mapped to which declarations want it — and
> journals every submit/withdraw as a revision. **Nothing is
> installed.** The agent does not touch the host. The whole point of
> this slice is to nail the bookkeeping model before any enforcement
> code goes near it.

## Build

```bash
cargo build --release -p golemd -p golemctl
```

(Static musl builds via `./build-static.sh` still work if you want
them; not required for local hacking.)

## Run the agent

```bash
./target/release/golemd --node dev-01 \
  --state-dir /tmp/golem-state \
  --listen 127.0.0.1:7474
```

State lives in `/tmp/golem-state/state.db` (SQLite, WAL). Removing
that directory resets the node.

## A Declaration in Nickel

The whole input language is one record:

```nickel
let g = import "../../nickel/lib.ncl" in
{
  name     = "web",
  packages = ["nginx", "curl", "git"],
} | g.Declaration
```

That's it. A `name`, a list of `packages`. Three canonical
declarations are in `examples/canonical/` — `base`, `web`,
`monitoring`. Their package sets overlap on purpose so you can watch
the refcounting work.

## Submit and inspect

```bash
# Submit each declaration.
./target/release/golemctl submit examples/canonical/base.ncl       http://127.0.0.1:7474
./target/release/golemctl submit examples/canonical/web.ncl        http://127.0.0.1:7474
./target/release/golemctl submit examples/canonical/monitoring.ncl http://127.0.0.1:7474
```

Each submit returns the new revision (id, kind, actions, resolved
state).

```bash
./target/release/golemctl state http://127.0.0.1:7474
```

→ canonical state. Each package lists the declarations that want it:

```json
{
  "packages": {
    "curl":                     ["base", "monitoring", "web"],
    "git":                      ["base", "web"],
    "nginx":                    ["web"],
    "prometheus-node-exporter": ["monitoring"]
  }
}
```

## Withdraw, watch refcounts work

```bash
./target/release/golemctl withdraw web http://127.0.0.1:7474
./target/release/golemctl state    http://127.0.0.1:7474
```

`nginx` is gone (only `web` wanted it). `curl` stays (base + monitoring
still do). `git` stays (base still does).

```json
{
  "packages": {
    "curl":                     ["base", "monitoring"],
    "git":                      ["base"],
    "prometheus-node-exporter": ["monitoring"]
  }
}
```

## The journal

Every submit/withdraw is a revision. The first revision is `init`
(empty state, written when the node boots for the first time).

```bash
./target/release/golemctl history http://127.0.0.1:7474
./target/release/golemctl show    http://127.0.0.1:7474 3
```

Each revision carries:

- `id`, `at`, `kind` (`init` / `submit` / `withdraw`)
- `declaration` — which declaration changed (null for `init`)
- `actions` — what would have to happen at the system level
  (`{"kind":"install","package":"nginx"}` etc.) to transition from the
  previous revision to this one. **These are recorded, not executed.**
- `state` — the full resolved state at this revision

So you can answer "what was the state at revision 5?" with one HTTP
call, and "what actions did revision 6 record?" with another.

## Manual HTTP, if you prefer

```bash
curl -X POST http://127.0.0.1:7474/declarations \
  -H 'content-type: application/json' \
  -d '{"name":"adhoc","packages":["htop","jq"]}'

curl    http://127.0.0.1:7474/state | jq
curl    http://127.0.0.1:7474/revisions | jq
curl -X DELETE http://127.0.0.1:7474/declarations/adhoc
```

## What this isn't (yet)

- **No enforcement.** Nothing reaches apt, systemd, the filesystem,
  the network. Adding enforcement layers on top of this — the next
  pass once the bookkeeping model is settled.
- **No signing.** Anyone who can reach the agent's port can submit
  declarations. Trust is its own concern; we'll bolt ed25519 back on
  when it's needed, and possibly migrate the wire format off JSON at
  the same time (see CLAUDE.md).
- **No multi-node.** One agent, one node, one URL. The `--node`
  argument is just for `/status`.
- **No claim kinds beyond packages.** A Declaration's `packages` field
  is a list of strings; the agent doesn't try to parse them as apt /
  brew / anything. They're labels.
