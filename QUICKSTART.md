# Quickstart

> **The model.** You author a fleet in **Emet**, and `emetc` runs your program
> to completion on your own machine — every function applied, every value
> computed — then writes the result as a binary, content-addressed **manifest**
> (`format_version` 5) holding one **scroll** per host.
>
> A scroll is a tree. Its leaves hold **glyphs**, one OS resource each, over
> four kinds: `aptPackage`, `systemdService`, the filesystem glyph
> (`file`, `directory`, `symlink`), and `lineInFile`. Each leaf is enacted as
> its own unit, with its own retries and its own rollback; the branches above
> them group leaves by subsystem and hand policy down.
>
> A per-host `golemd` ingests the manifest, selects its own scroll, diffs it by
> content id, and enacts the difference through reversible reconcilers,
> journalling what it did so every change can be undone. By default `golemd`
> runs the **fake** reconciler, which records intent without touching the
> host — safe to run anywhere.

## Build

```bash
nix build
```

All four binaries land in `./result/bin`: `emetc` (the `emet` crate's compiler,
which `golemctl apply` invokes on a `.emet` source), `emet-lsp`, `golemd`, and
`golemctl`. Each is static-musl, so the same file runs on a Debian guest and on
NixOS. Every command below is written against `./result/bin`.

In the devenv shell, `build-all` runs that plus the docs site, which lands in
`./result-site`. `cargo build --workspace` remains the fast inner loop while
editing Rust — the shell puts `target/release` first on `PATH`, so a fresh cargo
build wins inside this checkout.

### Tools on `PATH` everywhere

`./result/bin` only helps where you built. An editor opened on another repo
spawns `emet-lsp` by bare name and finds nothing, so install into your nix
profile instead:

```bash
nix profile install ~/path/to/golem#golem-tools   # `install-tools` in the devenv shell
nix profile remove golem-tools                    # `uninstall-tools`
```

Name the `golem-tools` attribute rather than the bare flake path. A profile
element takes its name from the flake reference it came from, so `nix profile
install .` registers it under the checkout's directory name and `nix profile
remove golem-tools` then matches nothing. Re-run the install to pick up new
commits.

**Syntax highlighting is not part of this.** The tree-sitter grammar for `.emet`
lives in the separate [emet.nvim](https://github.com/dull-ca/emet.nvim) repository
and is installed by nvim itself
(`:TSInstall! emet`). No nix output here provides it, and a working `emet-lsp`
will not colour a buffer on its own.

## Run the agent

```bash
./result/bin/golemd --host dev-01 \
  --state-dir /tmp/golem-state \
  --listen 127.0.0.1:7474
```

`--host` names which scroll this node enacts. State lives in
`/tmp/golem-state/planroom.db` (SQLite, WAL); removing that directory resets the
node. Add `--reconciler host` to enact for real (apt/systemd/file); the default
is `--reconciler fake`.

`--config golemd.toml` points at a config file with four tables. `[retry]` sets
the fleet-wide retry pace (backoff, jitter, attempt and wall-time limits), which
a per-scroll `policy` overrides. `[enact] workers` sets how many leaf units the
node enacts at once (4 by default; `1` is serial). `[auth] token_file` names the
shared bearer secret and `[secrets] key_file` the fleet secret key — both
covered below, and both overridden by their flags. Absent, built-in defaults
apply.

## Authorize the agent

golemd's port is root-equivalent control of its host, so a deployed daemon binds
loopback and requires a shared secret on every request (ADR 0042):

```bash
install -m 0600 /dev/null /etc/golem/token            # 0600 before a byte is in it
head -c 32 /dev/urandom | base64 > /etc/golem/token   # the redirect keeps that mode
./result/bin/golemd --host dev-01 \
  --listen 127.0.0.1:7474 \
  --auth-token-file /etc/golem/token
```

`[auth] token_file = "/etc/golem/token"` in `golemd.toml` says the same thing;
the flag wins. Every route then answers only to `Authorization: Bearer <token>`
and returns `401` otherwise. golemctl takes that token from the first of: a
host's inventory `token_file`, `GOLEM_AUTH_TOKEN`, or the file named by
`GOLEM_AUTH_TOKEN_FILE`:

```bash
export GOLEM_AUTH_TOKEN_FILE=~/.config/golem/token
```

**With no token file configured, golemd answers anyone who reaches the port.**
That is the local-development posture and nothing else; every example below
that dials `127.0.0.1:7474` assumes it.

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
distinct units on one host, nest them with `groups` — a glyph that wants to sit
beside a group gets its own one-glyph leaf, so every level is one of the two: a
leaf holding `glyphs`, or a branch holding `groups`.

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

A leaf is the **unit of failure isolation**: each one retries and settles on its
own, and a failure stops at that boundary while its siblings carry on. A scroll
may carry an optional `policy` — `rollback` (the default),
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

## Secrets

A password, an API token, or any other value that must not sit in source control
is written as `Secretspec.get "KEY"`, whose type is `String`. `emetc` resolves it
through secretspec while your program runs and seals it into the manifest under
a **fleet key**, so the manifest can be stored, cached, and handed to CI like any
other build output (ADR 0047).

Declare each key in a `secretspec.toml` in the entry file's directory or any
directory above it, and then use it like the `String` it is:

```toml
# examples/limesurvey/secretspec.toml
[project]
name = "limesurvey"
revision = "1.0"
require_reason = false

[profiles.default]
LIMESURVEY_DB_PASSWORD = { description = "MariaDB root password backing LimeSurvey" }
```

```elm
dbPassword : String
dbPassword = Secretspec.get "LIMESURVEY_DB_PASSWORD"

config =
  file
    { path = "/etc/app/app.conf"
    , contents = "password=${dbPassword}\n"
    , mode = "0600"
    }
```

**`mode = "0600"`, and a `file` glyph.** A `file` whose contents carry a secret
must keep it to its owner: `emetc` refuses any mode granting group or other
read, naming the path and the mode you wrote. `lineInFile` refuses a secret
outright — it owns one line and not the file it appends to, so it can promise
nothing about who may read the result. A key that is not declared in
`secretspec.toml` is a compile error listing the ones that are, raised before
any provider is consulted.

The fleet key is 64 bytes, written as hex. Generate it the way you generate the
bearer token, and keep the same mode:

```bash
install -m 0600 /dev/null /etc/golem/secret-key            # 0600 before a byte is in it
head -c 64 /dev/urandom | od -An -tx1 | tr -d ' \n' > /etc/golem/secret-key
```

`emetc` seals with it and every host's golemd unseals with it, so both ends need
the same file:

```bash
./result/bin/emetc build --secret-key /etc/golem/secret-key examples/limesurvey/main.emet -o limesurvey.manifest

./result/bin/golemd --host manta \
  --listen 127.0.0.1:7474 \
  --secrets-key-file /etc/golem/secret-key
```

`[secrets] key_file = "/etc/golem/secret-key"` in `golemd.toml` says the same
thing; the flag wins. `golemctl apply` runs `emetc` for you and passes it no
secret flags, so point the compiler at the key through the environment instead:

```bash
export GOLEM_SECRET_KEY_FILE=~/.config/golem/secret-key
./result/bin/golemctl apply examples/limesurvey/main.emet ssh://golem@manta
```

Provider and profile come from `--secret-provider` / `--secret-profile`, and
from secretspec's own `SECRETSPEC_PROVIDER` and `SECRETSPEC_PROFILE` where the
flag is absent.

Sealing is deterministic: the same secret yields the same bytes and the same
content id, so a rebuild re-enacts nothing, while a rotated secret yields new
bytes and golem re-enacts exactly the units that depend on it. Rotating the
fleet key itself means recompiling every manifest that carries a secret — a
manifest sealed to the old key is undecodable by the new one, and golemd reports
the mismatch by glyph rather than enacting a stale credential. A host with no
key configured enacts any manifest carrying no secret and refuses, by name,
every glyph carrying one.

**This protects the artifact, not the box.** golemd decrypts to write the file,
and the host holds the plaintext afterwards exactly as it would have anyway.

## Plan, apply, inspect

`golemctl plan` shows what an apply *would* do without doing it — the node
diffs the manifest against its journal and returns the ordered operations,
collapsed one line per action, with the coalesced reload step last (ADR 0036):

```bash
./result/bin/golemctl plan examples/lichess/fleet.emet http://127.0.0.1:7474
```

Add `--detail` for per-glyph content ids, `--json` for the raw response. A
plan never writes anything and is safe to run while an apply is in flight.

`golemctl apply` takes the same `.emet` source (it runs `emetc build` for you)
or a prebuilt `.manifest`, and POSTs the manifest bytes to the node:

```bash
./result/bin/golemctl apply examples/lichess/fleet.emet http://127.0.0.1:7474
```

The node selects the scroll named for its `--host`, answers `202` with a
`reconcile_id`, and runs the reconcile detached. `golemctl apply` polls that id
and renders what settled and what failed: a live per-unit tree on a terminal,
deterministic plain lines under `--json` or a pipe. `--reattach` skips the POST
and picks the newest attempt back up when a connection drops. A partial or
rolled-back reconcile is a result, not a transport error — the report still
prints, and the exit code is nonzero so a caller can branch on it.

```bash
./result/bin/golemctl state   http://127.0.0.1:7474   # current applied scroll + content id
./result/bin/golemctl history http://127.0.0.1:7474   # the revision journal
./result/bin/golemctl show    http://127.0.0.1:7474 3 # one revision by id
```

A remote daemon is not dialed directly — it is loopback-bound. Write the target
as `ssh://[user@]host[:port]` and golemctl opens its own forward over ssh, then
speaks HTTP through it (ADR 0042):

```bash
./result/bin/golemctl plan  examples/lichess/fleet.emet ssh://golem@scaly
./result/bin/golemctl apply examples/lichess/fleet.emet ssh://golem@scaly:2222
```

The port in an `ssh://` target is **ssh's**, not golemd's; the daemon is assumed
to be on `7474` behind it. Your own `~/.ssh/config` applies — an existing
ControlMaster makes the forward cost milliseconds. The tunnel is the encryption
and the host authentication: golem has no TLS of its own.

## Apply to a whole fleet

A manifest already carries every host's scroll, so applying it everywhere is one
command over a declared set of endpoints. That set is a TOML **inventory**
naming each host and saying how to reach the golemd that serves it:

```toml
# fleet.toml
[hosts]
scaly = "http://127.0.0.1:7474"          # a directly dialed daemon

[hosts.manta]
url = "http://127.0.0.1:7475"            # the same, written as a table

[hosts.orbit]                            # the deployed shape
ssh         = "golem@10.0.0.5"           # where to ssh
ssh_port    = 2222                       # ssh's port; default is ssh's own
remote_port = 7474                       # golemd's loopback port on that host
ssh_args    = ["-i", "/keys/id_ed25519"] # extra flags for the ssh command
token_file  = "/keys/golem-token"        # this host's secret, overriding the env
```

A bare string and a table with `url` say the same thing: dial this address. That
form fits a daemon you can actually reach on the address you wrote — a golemd
you started yourself, one per loopback port. A table with `ssh` says the daemon
is loopback-bound and golemctl must open its own forward (ADR 0042); `url` and
`ssh` together is an error, since a host is reached one way. Only `ssh` is
required in that form — the rest have defaults. Every guest the VM harness boots
is the `ssh` form: golemd binds loopback inside the guest and no port is
forwarded to it (`apps/fleet/README.md`).
golemctl looks for the file at `--inventory`, then `$GOLEMCTL_INVENTORY`, then
`./fleet.toml`, then `./.fleet/inventory.toml` (the file the VM harness writes).

```bash
./result/bin/golemctl fleet plan   examples/lichess/fleet.emet
./result/bin/golemctl fleet apply  examples/lichess/fleet.emet
./result/bin/golemctl fleet status
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
still runs to its terminal phase and is reported. That covers credentials too —
one fan-out opens one tunnel per ssh host and presents each host's own token, so
a host that refuses the secret errors by itself.

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

The API is plain HTTP + JSON, so curl reaches a gated daemon as well as golemctl
does — it just has to carry the header itself, and to be on the loopback the
daemon listens on.

Write the header into a file of its own and let curl read it from there
(`-H @file`, curl 7.55+). A header passed as `-H "Authorization: Bearer $TOKEN"`
is in curl's argv, and every argv on the box is world-readable in `/proc` for as
long as the process runs — which would hand the fleet secret to any local user
who happens to run `ps` at the right moment:

```bash
install -m 0600 /dev/null /etc/golem/auth-header
{ printf 'Authorization: Bearer '; cat /etc/golem/token; } > /etc/golem/auth-header

curl -H @/etc/golem/auth-header -X POST http://127.0.0.1:7474/manifest \
  --data-binary @examples/lichess/fleet.manifest       # 202 {"reconcile_id": N}

curl -H @/etc/golem/auth-header http://127.0.0.1:7474/reconciles/N | jq
curl -H @/etc/golem/auth-header http://127.0.0.1:7474/state        | jq
curl -H @/etc/golem/auth-header http://127.0.0.1:7474/revisions    | jq
```

`POST /manifest` returns as soon as the manifest is ingested; the reconcile runs
detached and `/reconciles/<id>` (or `/reconciles/latest`) is how you watch it.
A second POST while one is running answers `409` naming the reconcile in flight.

Drop the header only against an ungated dev daemon; a gated one answers `401`
with a message naming the flag. For a remote host, open the forward yourself and
curl through it — this is exactly what an `ssh://` target does for you:

```bash
ssh -N -L 127.0.0.1:7474:127.0.0.1:7474 golem@scaly &
curl -H @/etc/golem/auth-header http://127.0.0.1:7474/status | jq
```

## What this isn't (yet)

- **Fake by default.** `--reconciler fake` records intent without touching apt,
  systemd, or the filesystem, so a golemd you have just started changes nothing
  until you ask it to. `--reconciler host` enacts for real: it is what the
  `apps/fleet` harness runs on every Debian guest it boots, and what
  `TUTORIAL-fleet.md` walks through end to end.
- **One check, and the rest is the infrastructure's.** Submitting a change takes
  membership in two sets at once: people who can ssh to the box, and people who
  hold the shared secret (ADR 0042). golemd binds loopback, golemctl rides the
  operator's existing ssh connection, and the daemon's whole authorization model
  is one bearer-token comparison. Everything else — confidentiality, host
  authenticity, reachability — belongs to the layer below (ssh, segmentation,
  unix sockets with file permissions, a mesh VPN), which golem can provision
  like anything else (ADR 0040). Binding `--listen` to a routable interface
  publishes root-equivalent control of that host, and running with no token file
  publishes it to whoever can reach the port.
- **No per-user identity.** The secret says *someone authorized* did this, not
  who. Per-user tokens, an audit trail of who submitted, and SSO group mapping
  arrive with the authentik follow-on; the gate is one header check precisely so
  that swap changes the middleware and not the architecture.
- **No daemon-side coordination.** One agent per host, each enacting only its
  own scroll from the shared manifest. `golemctl fleet` fans out from the
  operator's machine, so that machine must be able to ssh to every host and hold
  each one's secret; golem-to-golem
  propagation — submit to one, all receive — is designed but unbuilt (ADR 0039).
- **Four glyph kinds only.** golemd reconciles apt packages, systemd units,
  filesystem entries, and lines in files. Richer shapes — workloads, services,
  ingress — are Emet libraries that lower onto those four, which is what keeps
  the agent this small.
