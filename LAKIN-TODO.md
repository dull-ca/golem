# Fleet demo runbook — the dogfood loops

Copy-paste sequences (nushell, one command per line, run from the repo root
inside the devenv shell). Everything is idempotent to re-run; skip
`fleet reset` to keep existing VM state.

**Every loop below already runs over SSH with a bearer token.** There is no
other mode: a deployed golemd binds `127.0.0.1:7474` and answers only to
`Authorization: Bearer <token>` (ADR 0042). `deploy` generates
`.fleet/golem-token`, installs it 0600 at `/etc/golem/token` on each guest,
and every harness verb opens its own ssh forward and presents it. Nothing
below needs a flag or an environment variable — if a command works, the
secured path is what carried it. Demo 0 is the loop that proves that.

## Demo 0 — the secured path (run this first, and after any auth change)

```nushell
fleet up
fleet deploy
fleet status
fleet inventory
golemctl fleet status
```

`fleet status` reaches every guest through a tunnel; `golemctl fleet status`
does the same with zero configuration — it finds `.fleet/inventory.toml` last
in its resolution chain, reads each host's `ssh`/`ssh_port`/`ssh_args`, and
takes the token from that host's `token_file`. Six `·` lines with no flags
set is the whole posture working end to end.

Now prove each layer separately. The direct golemd port must be dead:

```nushell
curl -m 5 http://127.0.0.1:8859/status
```

Expect a connection failure (exit 7/56), not a reply — the `88xx` forwards
still exist in qemu but nothing listens on the guest side of them any more.
Then open a forward by hand and try it bare, wrong, and right:

```nushell
ssh -f -N -L 17501:127.0.0.1:7474 -i .fleet/id_ed25519 -p 2259 -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ExitOnForwardFailure=yes golem@127.0.0.1
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:17501/status
curl -s -o /dev/null -w '%{http_code}\n' -H 'Authorization: Bearer wrong' http://127.0.0.1:17501/status
curl -s -H $"Authorization: Bearer (open .fleet/golem-token | str trim)" http://127.0.0.1:17501/status
pkill -f "17501:127.0.0.1:7474"
```

Expected: `401`, `401`, then `{"host":"scaly","latest_revision":N}`. The 401
body names the fix (`missing or invalid bearer token — golemd requires
Authorization: Bearer <token>`).

For a single-host verb against a VM, use the inventory — it carries the key
and ssh options each guest needs:

```nushell
golemctl fleet status --hosts scaly
```

The bare `ssh://` form takes no ssh options (it is the production shape,
where the destination is a real host your ssh config already knows), so
against the VM fleet it fails with `Host key verification failed` unless you
teach ssh about the guest first. Add this to `~/.ssh/config` if you want the
production spelling locally:

```
Host golem-scaly
  HostName 127.0.0.1
  Port 2259
  User golem
  IdentityFile ~/personal-repos/golem/.fleet/id_ed25519
  StrictHostKeyChecking no
  UserKnownHostsFile /dev/null
  ControlMaster auto
  ControlPath ~/.ssh/cm-%r@%h:%p
  ControlPersist 10m
```

then, with the token in the environment (a bare `ssh://` carries no
`token_file`):

```nushell
$env.GOLEM_AUTH_TOKEN_FILE = ($env.PWD | path join .fleet golem-token)
golemctl state ssh://golem-scaly
```

`ControlPersist` is what makes the second and later calls instant — golemctl
spawns your `ssh`, so an existing master is reused for free.

Rotation is one file: delete `.fleet/golem-token` and re-run `fleet deploy`.
The token deliberately survives `fleet reset` and even `reset --purge`, so a
rebuilt fleet keeps talking to an inventory you already rendered.

**If you want to see it fail:** point golemctl at a routable address with a
token in the environment and it warns before sending (`… is not loopback —
the auth token crosses that network in cleartext`). That warning is the only
guard there — golem lets you do it, it just never does it silently.

## Demo 1 — the grouped fishnet farm (per-unit isolation, keep-policy canary)

```nushell
fleet reset
fleet up --hosts scaly
fleet deploy --hosts scaly
fleet plan examples/fishnet-farm/farm.emet
fleet apply examples/fishnet-farm/farm.emet
fleet apply examples/fishnet-farm/farm.emet
```

First apply: six units settle green, the canary fails its retries and is
**kept** (`partial`), with the pull error in the forensics block under the ✗
line. The second apply proves the canary honestly resurfaces every reconcile
instead of vanishing into "unchanged".

A cold host's first apply runs many minutes (apt update, podman install, the
fishnet pull) — `fleet apply` now waits indefinitely. If the connection does
drop, golemd keeps reconciling server-side: watch `fleet logs scaly -f` and
re-run the apply for the report. (Live progress during the wait is coming —
ADR 0033.)

On-box journal access works without sudo on fresh VMs:

```nushell
fleet ssh scaly -- journalctl -u fishnet-move-1.service -n 5
```

## Demo 2 — talos's full stack (and scaly's regroup)

```nushell
fleet up --hosts talos
fleet deploy --hosts talos
fleet plan examples/lichess/fleet.emet
fleet apply examples/lichess/fleet.emet
```

Watch scaly convert farm → flat lichess scroll (13 removes under
`scaly / <removes>`, reverse teardown order) while talos stands up fishnet,
influxdb, prometheus, alertmanager, loki, dns (on 5353 — systemd-resolved owns
53 on the guests), ntp, rsyslog, and the nginx ingresses. The first run pulls
~8 images; expect minutes.

## Demo 3 — the website CI loop (build → push → pull → serve)

```nushell
fleet up --hosts registry,builder,web --publish registry=5000:5000 --publish web=8081:80
fleet deploy --hosts registry,builder,web
fleet plan examples/registry/registry.emet --hosts registry
fleet apply examples/registry/registry.emet --hosts registry
fleet plan examples/website/builder.emet --hosts builder
fleet apply examples/website/builder.emet --hosts builder
cd sites/website
bun run build
cd ../..
tar -C sites/website -cf - Containerfile Caddyfile dist | fleet ssh builder -- "mkdir -p site && tar -xf - -C site"
fleet ssh builder -- "sudo podman build -t 10.0.2.2:5000/golem-website:latest site && sudo podman push 10.0.2.2:5000/golem-website:latest"
fleet plan examples/website/website.emet --hosts web
fleet apply examples/website/website.emet --hosts web
curl -sI http://127.0.0.1:8081/ | lines | first
```

Expected finish: `HTTP/1.1 200 OK`, serving `<title>Golem | Golem</title>` —
golem's docs, built on a golem-provisioned builder, stored in golem's own
registry, served by a golem-managed web box.

Notes:
- The quoted `&&` chains run in the guest's bash over ssh — fine from nushell.
- Web publishes on 8081 because the host's 8080 was held by an ssh tunnel;
  any free port works (`--publish web=PORT:80`).

## Testing the LSP and tree-sitter grammar

Full instructions live in the two READMEs; the short version:

**emet-lsp** (`apps/emet-lsp/README.md` — has both nvim config styles):

```nushell
cargo build -p emet-lsp --release
```

Point nvim at `target/release/emet-lsp` using the README's autocmd or
lspconfig snippet, open any `.emet` file (e.g. `examples/fishnet-farm/farm.emet`),
and break something — diagnostics (parse/type/analyze, the friendly ADR 0032
messages) appear inline on every edit. Hover, completion, and goto-definition
are wired per ADR 0018.

**tree-sitter-emet** (the grammar now lives in its own repo,
[`emet.nvim`](https://github.com/dull-ca/emet.nvim), checked out at
`../emet.nvim`):

```nushell
cd ../emet.nvim
tree-sitter test
tree-sitter parse ../golem/examples/fishnet-farm/farm.emet
cd ../golem
```

`tree-sitter test` runs the corpus (`test/corpus/basics.txt`); `parse` shows
the tree for any file. The website's syntax highlighting is the TextMate
grammar generated FROM this tree-sitter grammar (regenerated in `emet.nvim` via
`node scripts/generate-textmate.mjs` and committed here at
`sites/website/src/grammars/emet.tmLanguage.json`).

**LazyVim editor highlighting** — one line, since `emet.nvim` bundles the plugin
(parser config, filetype, and queries). Drop this in
`~/.config/nvim/lua/plugins/emet.lua`:

```lua
return {
  { "dull-ca/emet.nvim" },
}
```

and `:TSInstall emet` inside nvim. Full details: `../emet.nvim/README.md`.

## Handy while things run

```nushell
fleet status
fleet logs scaly -f
curl -s http://127.0.0.1:5000/v2/_catalog
```

`fleet logs` and `fleet ssh` are plain ssh to the guest, unaffected by the
auth layer. The registry curl above works because `--publish` forwards that
port itself — only golemd is loopback-only.

## When the secured path breaks

- **`unreachable: … Connection refused`** on one host — its golemd is down or
  never deployed; `fleet logs <host>` (plain ssh) still works, so read it
  there. A guest deployed before ADR 0042 is still listening on `0.0.0.0`
  with no token: `fleet deploy --hosts <host>` fixes it.
- **`401` from every host** — the guests hold a different secret than
  `.fleet/golem-token`. Deleting the local file regenerates a *new* one, so
  after any rotation `fleet deploy` must run everywhere before anything else
  will answer.
- **A verb hangs ~10s per host, then reports an ssh error** — the forward
  never opened. The message carries ssh's own stderr; the usual causes are a
  stopped VM or a stale `.fleet/inventory.toml` (re-run `fleet inventory`
  after every `up`, `down`, or `reset` — it is a snapshot, not a live view).
- **`no inventory at …`** — you are outside the repo root, or never ran
  `fleet inventory`. golemctl looks at `--inventory`, then
  `$GOLEMCTL_INVENTORY`, then `./fleet.toml`, then `./.fleet/inventory.toml`.

## Still open (2026-08-01)

- Per-user identity: the token is per *fleet*, so revocation is rotating one
  file and nothing records who submitted a change. Authentik + lichess SSO is
  the recorded follow-on (ADR 0042) and swaps the middleware, not the
  architecture.
- A local process on this workstation can win golemctl's forward port between
  ssh's spawn and its bind, and would receive the token. Accepted and written
  down; forwarding a unix socket instead of a TCP port is the race-free fix
  if that assumption ever weakens.
- golem-to-golem propagation (submit to one, all receive) is designed but
  unbuilt — ADR 0039, whose precondition is golemd adopting the fleet's
  absence-is-silence rule daemon-side.
