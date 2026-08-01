<!--
  COUNTERPART: this file is duplicated on the docs site under
  sites/website/src/content/docs/. Edit one and edit the other.

    Before you start + Lesson 1 + Reading the fleet + When it breaks
                        → tutorials/the-vm-harness.mdx   (/tutorials/the-vm-harness/)
    Lesson 2 + Lesson 3 → tutorials/a-failing-unit.mdx   (/tutorials/a-failing-unit/)
    Lesson 4 steps 1–4  → tutorials/registry-on-the-fleet.mdx
    Lesson 4 steps 2–5  → tutorials/website-loop.mdx
    Appendix            → explanation/the-fleet-harness.mdx

  "Working on the language while the fleet runs" and "Still open" are
  repo-only — they are contributor notes, not published docs.
-->

# Running golem against real machines

By the end of this you will have Debian trixie VMs on your workstation, each
running `golemd` against real apt, real systemd, and a real filesystem. You
will have watched one deliberately broken unit fail while its eight siblings
settle green. You will have built golem's own documentation on a
golem-provisioned builder, stored the image in a golem-provisioned registry,
and served it from a golem-provisioned web box — and read it back over HTTP.

And every byte of that will have crossed an ssh forward carrying a bearer
token, because there is no other way in. Lesson 1 is where you prove that
rather than take it on faith.

Commands are nushell, one per line, run from the repo root inside the devenv
shell. All of them are safe to re-run.

---

## Before you start

```nushell
cargo build --workspace
cargo build --release -p golemctl
```

The workspace build gives `fleet` its `emet` compiler and its
`target/debug/golemctl`. The release build is what puts `golemctl` on the
devenv shell's `PATH` (`enterShell` prepends `target/release`), which Lesson 1
step 7 depends on.

Two waits are worth knowing about before you think something has hung:

- The first `fleet up` on a machine downloads Debian's ~340 MB genericcloud
  image, then waits out cloud-init on each guest. Minutes.
- The first `fleet deploy` runs `nix build .#golemd-static`. Minutes again,
  unless cachix already has it.

Reference for everything below: `QUICKSTART.md` for the model and the full verb
surface, `apps/fleet/README.md` for the harness, `docs/adr/0042` for the
transport and auth decision this tutorial exercises.

---

## Lesson 1 — Bring the fleet up, and prove nothing else can reach it

### 1. Boot six guests

```nushell
fleet up
```

Per host you get two lines:

```
Booting scaly (ssh 2259, golemd loopback-only)…
  scaly up pid=482913
```

Read `golemd loopback-only` literally. `2259` is scaly's forwarded ssh port, and
for a guest booted this way it is the only port qemu forwards. There is no
golemd port in the line because there is no golemd port.

### 2. Install the daemon

```nushell
fleet deploy
```

```
Building static golemd (musl)…
  binary: /home/you/personal-repos/golem/.fleet/result-golemd-static/bin/golemd
Deploying golemd to scaly…
  scaly: golemd up {'host': 'scaly', 'latest_revision': 1}
```

That `latest_revision: 1` is the `init` revision golemd writes on first boot —
an empty scroll, nothing applied yet.

### 3. Look at the fleet

```nushell
fleet status
```

A table, one row per VM: `up`, `golemd reachable`, and dashes under
`content-id` and `glyphs` because nothing has been applied. Every one of those
`reachable` cells was answered through an ssh forward the harness opened and
tore down for that single request.

### 4. Try the port that used to exist

```nushell
curl -m 5 http://127.0.0.1:8859/status
```

```
curl: (7) Failed to connect to 127.0.0.1:8859 after 0 ms: Could not connect to server
```

Exit 7, refused before a packet leaves the machine. Having the fleet up changes
nothing. `8859` is the port scaly's name-derived slot earns (`8800 + 59`), and
it is still recorded in `.fleet/state.json`, but the only host→guest forward
qemu creates is ssh — plus whatever you ask for with `--publish`, which Lesson 4
uses and which has nothing to do with golemd. The hostfwd that once pointed at 8859 was
deleted when the loopback bind made it unreachable by construction, so there is
no longer a port to knock on — not a dead one, none.

### 5. Open a forward yourself, and get refused twice

```nushell
ssh -f -N -L 17501:127.0.0.1:7474 -i .fleet/id_ed25519 -p 2259 -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ExitOnForwardFailure=yes golem@127.0.0.1
curl -s -w ' [%{http_code}]\n' http://127.0.0.1:17501/status
curl -s -w ' [%{http_code}]\n' -H 'Authorization: Bearer wrong' http://127.0.0.1:17501/status
curl -s -w ' [%{http_code}]\n' -H $"Authorization: Bearer (open .fleet/golem-token | str trim)" http://127.0.0.1:17501/status
pkill -f "17501:127.0.0.1:7474"
```

The three curls, in order:

```
{"kind":"unauthorized","message":"missing or invalid bearer token — golemd requires Authorization: Bearer <token> (see --auth-token-file)"} [401]
{"kind":"unauthorized","message":"missing or invalid bearer token — golemd requires Authorization: Bearer <token> (see --auth-token-file)"} [401]
{"host":"scaly","latest_revision":1} [200]
```

Reaching the socket buys nothing. The tunnel is the confidentiality and the
host authentication; the token is the authorization; both are required and
neither substitutes for the other.

> The third curl puts the fleet secret in `curl`'s argv, which `/proc` makes
> world-readable for the life of the process. That is tolerable for a token
> that lives in your own checkout and governs throwaway VMs. Against anything
> real, write the header to a 0600 file and use `curl -H @file` — `QUICKSTART.md`
> has the incantation.

### 6. Render the inventory

```nushell
fleet inventory
```

It prints one path: `/home/you/personal-repos/golem/.fleet/inventory.toml`. Open
it if you like. Each guest is an `[hosts.<name>]` table carrying `ssh`,
`ssh_port`, the fleet key and host-checking options as `ssh_args`, and
`token_file`. No `url`, because there is nothing to dial.

### 7. Drive it with no flags and no environment

```nushell
golemctl fleet status
```

```
· kaiju  rev 1  nothing applied
· manta  rev 1  nothing applied
· orbit  rev 1  nothing applied
· scaly  rev 1  nothing applied
· talos  rev 1  nothing applied
· zulip  rev 1  nothing applied
```

Six `·` lines, alphabetical, and not one flag or environment variable. golemctl
looked at `--inventory`, then `$GOLEMCTL_INVENTORY`, then `./fleet.toml`, then
`./.fleet/inventory.toml` — and the harness writes its file at the end of that
chain deliberately, so being in the repo root is the whole configuration. Each
of those six lines is a separate ssh forward, opened and closed, presenting the
token that host's `token_file` named.

That is the posture proved end to end. Everything from here on rides it.

> **The single-host spelling.** `golemctl state ssh://golem-scaly` is the
> production shape, and a bare `ssh://` target carries no ssh options at all —
> golemctl parses it into a destination and a port and hands ssh nothing else.
> Against a VM that fails with `Host key verification failed`, because each
> fresh guest presents a new key on a reused port. Teach ssh about the guest
> once:
>
> ```
> Host golem-scaly
>   HostName 127.0.0.1
>   Port 2259
>   User golem
>   IdentityFile ~/personal-repos/golem/.fleet/id_ed25519
>   StrictHostKeyChecking no
>   UserKnownHostsFile /dev/null
>   ControlMaster auto
>   ControlPath ~/.ssh/cm-%r@%h:%p
>   ControlPersist 10m
> ```
>
> then supply the token, since a bare `ssh://` carries no `token_file` either:
>
> ```nushell
> $env.GOLEM_AUTH_TOKEN_FILE = ($env.PWD | path join .fleet golem-token)
> golemctl state ssh://golem-scaly
> ```
>
> `ControlPersist` is why the second call and every one after it is instant:
> golemctl spawns your `ssh`, so an existing master is reused for free.
>
> If you would rather not touch `~/.ssh/config`, `golemctl fleet status --hosts
> scaly` gets you one host with no configuration at all — the inventory already
> holds the key and the options.

> **Rotation** is one file. Delete `.fleet/golem-token`, run `fleet deploy`
> again, and every guest gets the new secret. Deleting the file generates a
> *new* token on the next command, so the redeploy has to reach every guest
> before anything else will answer. The token survives `fleet reset` and even
> `reset --purge` on purpose, so a rebuilt fleet keeps talking to an inventory
> you already rendered.

> **If you want to see it complain**, point golemctl at a routable address with
> a token in the environment. It warns on stderr before sending: `warning: host
> X is dialed at Y, which is not loopback — the auth token crosses that network
> in cleartext; use an ssh:// target to keep it inside the tunnel (ADR 0042)`.
> That warning is the only guard. golem will do it; it just will not do it
> quietly.

---

## Lesson 2 — Break one unit and watch its siblings ignore it

`examples/fishnet-farm/farm.emet` is one host, `scaly`, as a tree of nine leaf
units: three `fishnet-move` clients, two `fishnet-analysis` clients, a
`lila-gif` workload, the nftables firewall, a `base` leaf, and a `canary` whose
image (`golem-example/does-not-exist:latest`) cannot be pulled. The canary
carries `policy = keep`.

Each guest holds 2 GB. If your workstation is feeling it, `fleet down manta`
(and orbit, kaiju, zulip) frees them; a later `fleet up --hosts manta` resumes
that guest off the same disk. Leave `scaly` and `talos` up — Lesson 3 wants
both.

### 1. Read the diff before enacting it

```nushell
fleet plan examples/fishnet-farm/farm.emet --hosts scaly
```

`fleet` compiles the scroll, reports the manifest it produced and which running
hosts it will reach, then golemctl prints the diff under a heading naming the
host and its address:

```
scaly  ssh://golem@127.0.0.1:2259
  against revision 1 · manifest <id>…
  …one collapsed line per action…
  N changes · N install, N replace, N remove · N unchanged
```

Nothing is written. A plan is safe to run while an apply is in flight.

### 2. Apply it

```nushell
fleet apply examples/fishnet-farm/farm.emet --hosts scaly
```

A live tree draws as the units settle. Eight leaves finish `✓`. The canary's
leaf finishes `✗` with the podman pull error in the forensics block beneath it,
and the run closes with a summary under scaly's heading:

```
scaly  ssh://golem@127.0.0.1:2259
  apply partial — revision 2 — scaly / canary: 1 glyph failed (kept)
```

`fleet apply` passes golemctl's exit code through, and `partial` is a nonzero
outcome — so this successful lesson exits 1. That is the point: eight units
settled, one did not, and golem said so instead of rolling the whole host back.

A cold guest's first apply runs many minutes — `apt update`, installing podman,
pulling fishnet. `fleet apply` waits as long as it takes and streams progress
while it does. If the connection to your workstation drops, golemd keeps
reconciling server-side; `fleet logs scaly -f` shows it, and re-running the
apply gets you the report.

### 3. Apply it a second time

```nushell
fleet apply examples/fishnet-farm/farm.emet --hosts scaly
```

Same ending. The canary resurfaces on every reconcile rather than disappearing
into "unchanged" — a kept failure stays visible, because a unit that has never
worked is not a unit that needs no attention.

### 4. Read the journal on the box

```nushell
fleet ssh scaly -- journalctl -u fishnet-move-1.service -n 5
```

No `sudo`. cloud-init put the `golem` user in `systemd-journal` on first boot.

---

## Lesson 3 — Regroup one host while standing another up

`examples/lichess/fleet.emet` emits six scrolls. Four of them name images that
are not publicly pullable; `scaly` and `talos` are the live-tested pair, so
name them explicitly.

```nushell
fleet up --hosts talos
fleet deploy --hosts talos
fleet plan examples/lichess/fleet.emet --hosts scaly,talos
fleet apply examples/lichess/fleet.emet --hosts scaly,talos
```

The first two lines are no-ops if you left `talos` running after Lesson 1, and
are what brings it back if you did not. The plan shows both halves of what
follows before the apply performs them.

`scaly` goes from the nine-leaf farm tree to a single flat scroll holding one
fishnet workload. Its farm units come down in the reverse of their install
order, each undone by the inverse golem recorded when it installed them — golem
removes only what golem put there.

`talos` stands up fishnet, influxd, alertmanager, prometheus, loki, dns, ntpd,
rsyslogd, and the three nginx ingresses. dns publishes on 5353 because
systemd-resolved already owns 53 on the guests. That is roughly eight images to
pull on a cold box; expect minutes.

> Drop `--hosts` and `fleet apply` targets every VM in the state file, which is
> safe but slower: golemctl skips any host the manifest names no scroll for and
> reports it skipped. That is the absence-is-silence rule — a manifest that
> forgot to mention a host must not decommission it. Decommissioning is an
> authored empty scroll, never an omission.

---

## Lesson 4 — Build, store, and serve golem's own docs

Three fresh guests, two of them publishing a port to your workstation.

### 1. Boot and provision the three boxes

```nushell
fleet up --hosts registry,builder,web --publish registry=5000:5000 --publish web=8081:80
fleet deploy --hosts registry,builder,web
fleet apply examples/registry/registry.emet --hosts registry
fleet apply examples/website/builder.emet --hosts builder
```

The registry scroll lowers to podman, a named volume, a quadlet container unit,
its service, and an internal nftables drop-in. The builder scroll is two glyphs:
podman, and a `registries.conf.d` fragment marking `10.0.2.2:5000` insecure so
podman will push to it over plain HTTP.

`web` publishes on 8081 only because 8080 was busy here; any free port works
(`--publish web=PORT:80`).

### 2. Build the site on your workstation

```nushell
build-site
```

The devenv script that runs `bun run build` in `sites/website`. Output lands in
`sites/website/dist`.

### 3. Ship the build context to the builder and push the image

```nushell
tar -C sites/website -cf - Containerfile Caddyfile dist | fleet ssh builder -- "mkdir -p site && tar -xf - -C site"
fleet ssh builder -- "sudo podman build -t 10.0.2.2:5000/golem-website:latest site && sudo podman push 10.0.2.2:5000/golem-website:latest"
```

The quoted `&&` chains run in the guest's bash over ssh, so nushell never sees
them.

### 4. Check that the registry took it

```nushell
curl -s http://127.0.0.1:5000/v2/_catalog
```

```
{"repositories":["golem-website"]}
```

That request went to your workstation's `127.0.0.1:5000`, which qemu forwards
into the registry guest. The builder reached the same registry from *inside* its
own guest at `10.0.2.2:5000` — the host gateway — which is how two SLIRP-isolated
guests trade an image with no shared network between them.

### 5. Serve it

```nushell
fleet plan examples/website/website.emet --hosts web
fleet apply examples/website/website.emet --hosts web
curl -sI http://127.0.0.1:8081/ | lines | first
```

```
HTTP/1.1 200 OK
```

Golem's documentation, built on a golem-provisioned builder, stored in a
golem-provisioned registry, pulled and served by a golem-provisioned web box.
`curl -s http://127.0.0.1:8081/` shows `<title>Golem | Golem</title>` in the
markup.

---

## Working on the language while the fleet runs

The VMs are unaffected by anything you do to the compiler, so this is a fine
thing to do while an apply grinds through an image pull.

**emet-lsp.** Build it, then point nvim at `target/release/emet-lsp` using one
of the config styles in `apps/emet-lsp/README.md` (built-in LSP client,
nvim-lspconfig, or vim plugin):

```nushell
cargo build -p emet-lsp --release
```

Open any `.emet` file — `examples/fishnet-farm/farm.emet` is a good one — and
break something. Parse, type, and analysis diagnostics appear inline on every
edit, in the Elm-style shape of ADR 0032. The server also advertises hover,
completion, and goto-definition (ADR 0018, ADR 0037); the README's capability
list has not caught up with that.

**tree-sitter-emet.** The grammar lives in its own repo,
[`emet.nvim`](https://github.com/dull-ca/emet.nvim), checked out alongside golem
at `../emet.nvim`:

```nushell
cd ../emet.nvim
tree-sitter test
tree-sitter parse ../golem/examples/fishnet-farm/farm.emet
cd ../golem
```

`test` runs the corpus at `test/corpus/basics.txt`; `parse` prints the tree for
any file. The docs site's syntax highlighting is a TextMate grammar generated
from this one — regenerated in `emet.nvim` with `node
scripts/generate-textmate.mjs` and committed here at
`sites/website/src/grammars/emet.tmLanguage.json`.

**Editor highlighting** is one line, since `emet.nvim` bundles the plugin
(parser config, filetype, and queries). In
`~/.config/nvim/lua/plugins/emet.lua`:

```lua
return {
  { "dull-ca/emet.nvim" },
}
```

then `:TSInstall emet`. Details in `../emet.nvim/README.md`.

---

## Reading the fleet while it works

```nushell
fleet status
fleet logs scaly -f
fleet ssh scaly
```

`fleet logs` and `fleet ssh` are plain ssh into the guest and are untouched by
the auth layer — golemd's token gates golemd's HTTP surface, not your shell.

---

## When it breaks

**`✗ <host>  unreachable: ssh to golem@127.0.0.1 exited 255 before the forward
opened — Connection refused`**
The guest is not running, or `.fleet/inventory.toml` names a port that no longer
belongs to it. `fleet status` says which; re-run `fleet inventory` after every
`up`, `down`, or `reset`, because it is a snapshot of the state file rather than
a live view.

**`✗ <host>  unreachable: …` on one host while the others answer**
That guest's golemd is down or was never deployed. `fleet logs <host>` still
works — it is plain ssh — so read the journal there. `fleet deploy --hosts
<host>` reinstalls and restarts it.

**`unauthorized — set GOLEM_AUTH_TOKEN or GOLEM_AUTH_TOKEN_FILE (or an inventory
host's token_file) to golemd's configured secret`**
The guests hold a different secret than `.fleet/golem-token`. Almost always a
half-finished rotation: run `fleet deploy` against every host, not just the one
that failed.

**A verb hangs about ten seconds per host, then reports an ssh error**
The forward never came up. The message carries ssh's own stderr. Stopped VM or
stale inventory, same fixes as above.

**`no inventory at … — golemctl looks at --inventory, then $GOLEMCTL_INVENTORY,
then ./fleet.toml, then ./.fleet/inventory.toml`**
You are outside the repo root, or you never ran `fleet inventory`.

**`error: the fleet token file … is empty`**
Delete it and redeploy everywhere. A truncated secret is refused rather than
silently degraded into an unauthenticated request.

---

## Still open (2026-08-01)

- **No per-user identity.** The token is per *fleet*, so revocation means
  rotating one file, and nothing records who submitted a change. Authentik with
  lichess SSO is the recorded follow-on (ADR 0042); because the gate is a single
  header check, it swaps the middleware and not the architecture.
- **An accepted local race.** golemctl picks a free local port, then hands the
  number to ssh, which binds it only after authenticating — while publishing the
  number in its world-readable `/proc` command line. A local process that wins
  that port would receive the token. The window is accepted (it takes a hostile
  account on your own workstation, which has already lost) and written down;
  forwarding a unix socket in an operator-only directory instead of a TCP port is
  the race-free replacement if that assumption ever weakens.
- **No golem-to-golem propagation.** Submit to one daemon, all receive, is
  designed and unbuilt (ADR 0039). Its precondition is golemd adopting the
  fleet's absence-is-silence rule daemon-side.

---

## Appendix — What `fleet` is actually doing

### It is standing in for real hosts, not simulating them

golem's unit tests drive the reconcilers through a fake host. That proves the
logic and touches nothing. `fleet` closes the gap: rootless-QEMU Debian trixie
guests, a real `golemd` running its **`host`** reconciler (`--reconciler host`
in the unit it writes, not the default `fake`), against real apt, real systemd,
and a real filesystem. This is where the bugs a fake host cannot express get
found — the ADR-0015 `daemon-reload`-before-enable fix came from here.

Nothing about the guests is special-cased. They are reached over ssh with a
bearer token because that is how production hosts are reached; the harness has
no privileged path that a real deployment would lack.

### The base image and first boot

Guests boot Debian's `genericcloud` qcow2 from `trixie/latest`. The harness
scrapes that index for the newest concrete `.qcow2`, downloads it once with a
resumable `curl --continue-at -` into a `.part` file that is only renamed on
success, and caches it in `.fleet/images/`.

genericcloud ships cloud-init and no cloud-vendor agents, so one read-only seed
ISO configures a guest completely. The `user-data` sets the hostname, creates a
passwordless-sudo `golem` user whose only credential is the fleet's injected
public key, disables ssh password auth and root login, and adds `golem` to
`systemd-journal` — which is why `journalctl -u <unit>` works on a guest without
`sudo`. The ISO is built with `cloud-localds`, falling back to
xorriso/genisoimage/mkisofs, and attached read-only.

qemu runs `-daemonize` with `-display none` and its serial console redirected to
`console.log`. Under `-daemonize` there is no terminal to attach a console to,
so `-nographic`'s serial-to-stdin wiring would be wrong; the log file is where
boot-time failures are legible.

### One overlay per guest

Each VM's disk is a copy-on-write qcow2 backed by the shared base image. Writes
land in the overlay, the base stays pristine and is reused by every guest, and
discarding a VM is deleting one directory. Each guest gets 2 GB of memory and
2 vCPUs, under `-enable-kvm -cpu host`.

### Ports come from names, not boot order

A host's slot is `blake2b(name) mod 100`; its ssh port is `2200 + slot`. Hashing
the *name* rather than its position in the boot list means `scaly` is always
2259 whether you booted it alone or sixth. Rebooting a subset does not shuffle
anyone's port, and a `~/.ssh/config` stanza you wrote last week still works.

| name     | slot | ssh   |
|----------|------|-------|
| scaly    | 59   | 2259  |
| manta    | 28   | 2228  |
| orbit    | 64   | 2264  |
| talos    | 19   | 2219  |
| kaiju    | 74   | 2274  |
| zulip    | 10   | 2210  |
| registry | 65   | 2265  |
| builder  |  3   | 2203  |
| web      | 52   | 2252  |

The slots are collision-free across that set. `8800 + slot` is still computed
and still recorded in `state.json` — it keys the name→slot map and older records
carry it — but nothing forwards to it and nothing listens behind it. Ports for an
already-created VM are read from its record, so a guest booted under an older
scheme keeps the ports it was given.

### What survives `down`, `reset`, and `reset --purge`

- **`down`** kills qemu (SIGTERM, escalating to SIGKILL after ~5s) and leaves the
  overlay disk, seed ISO, and state record in place. A later `up` on that name
  *resumes* it against the same disk on its recorded ports, so guest data
  survives. SIGTERM does not sync the guest's page cache, so run `sync` in the
  guest first if a very recent write must live.
- **`reset`** kills every VM, deletes every `vm-*/` directory and the state file.
  All guest data is gone. The cached image and the keypair stay, so the next `up`
  is fast.
- **`reset --purge`** additionally drops `images/` and the keypair, so the next
  `up` re-downloads and re-generates.

`.fleet/golem-token` survives all three. That is deliberate: a rebuilt fleet
keeps talking to an inventory you already rendered and a `~/.ssh` setup you
already wrote. Rotation is an explicit delete, never a side effect of tearing
VMs down.

### golemd: one static file, one root unit

`deploy` builds the `golemd-static` flake output — crane over `pkgsStatic` with
`-C target-feature=+crt-static`, linking musl libc, bundled sqlite, and the
rustls crypto into a single file. A nix-*dynamic* binary names its interpreter as
a `/nix/store` path and simply will not run on Debian; the static one is one
`scp` away from running anywhere.

Installation, per guest: `scp` to a per-deploy unique staging name under
`/home/golem` (a fixed `/tmp` name wedges every later deploy the moment a stale
copy survives under other ownership), `install -m 0755` into
`/usr/local/bin/golemd`, create `/etc/golem`, write the token, write
`golemd.toml`, write the unit, `daemon-reload`, `enable`, then `restart` —
restart rather than `enable --now`, so a redeployed binary actually replaces the
running process.

The unit runs as root with
`--listen 127.0.0.1:7474 --config /etc/golem/golemd.toml --reconciler host`.
The config file says one thing: where the bearer secret lives. Retry and enact
defaults are left to golemd, so the file states only what the harness had to
decide.

The token is written with `install -m 0600 /dev/null` first and filled second, so
it is never briefly world-readable — and neither the create nor the write goes
through the harness's usual error-reporting path, because `tee` echoes its stdin
and a failure message built from stdout would print the fleet secret to your
terminal.

The token is the *fleet's*, not the guest's: every guest gets the same secret,
which is exactly what lets one `golemctl fleet` run span all of them.

### Networking: SLIRP, `10.0.2.2`, and `--publish`

Every guest runs behind qemu user-mode (SLIRP) networking, isolated from its
siblings. Only ssh is forwarded in by default. Two facts make cross-guest
traffic possible:

- `up --publish` adds a host→guest forward. `--publish registry=5000:5000` binds
  your `127.0.0.1:5000` to the registry guest's `:5000`. A bare `--publish
  5000:5000` publishes on *every* booted host, which clashes the moment two
  share a host port — name the host for a single service. Forwards are recorded
  per VM, so a resumed guest re-forwards them, and a resume can add forwards the
  stopped guest lacked.
- In SLIRP every guest reaches the host at `10.0.2.2`.

Compose them and you get a host-gateway rendezvous: guest A reaches guest B's
*published* port at `10.0.2.2:<host_port>`, because the connection lands on your
loopback and qemu forwards it into B. That is the whole mechanism behind Lesson
4 — one golem-hosted registry shared across machines, no shared L2 segment.

### Inside `.fleet/`

Everything ephemeral lives under `.fleet/` at the repo root, so nothing escapes
the checkout and `reset` can wipe it wholesale.

| path | what it is |
|------|------------|
| `images/` | the cached Debian base image, shared by every overlay |
| `id_ed25519`, `.pub` | the fleet keypair; the public half is injected by cloud-init |
| `golem-token` | the shared bearer secret, mode 0600, created `O_EXCL` |
| `state.json` | which VMs exist: name, ports, qemu pid, disk/pidfile/console paths, published forwards |
| `inventory.toml` | the rendered golemctl inventory, written when you ask for it |
| `vm-<name>/` | one per guest: `disk.qcow2`, `seed.iso`, `user-data`, `meta-data`, `qemu.pid`, `console.log` |
| `result-golemd-static` | the nix out-link for the static build |

(`toolchain/` is a leftover from an earlier cross-compilation approach; nothing
in the current harness references it.)

The harness's own read-only calls — `fleet status`, `deploy`'s readiness poll —
open a forward, make exactly one authorized request, and tear it down. They are
occasional enough that a per-call forward costs less than a long-lived one per
guest. `fleet apply` and `fleet plan` do not use that path at all: they render a
*fresh* per-run inventory into a temp directory and run `golemctl fleet
apply|plan --inventory` with inherited stdio — the TUI has to own the terminal
to draw its frames. golemctl opens every forward, holds the hosts concurrently,
and draws one live tree across them. The per-run file is separate
from `.fleet/inventory.toml` on purpose — that file is yours, written when you
ask, and may name a different set of guests than the invocation targets.

### A guest's name is its scroll's name

There is no mapping table anywhere, and that is the design. A VM named `scaly`
gets an inventory entry `[hosts.scaly]`; the manifest carries a scroll named
`scaly`; golemctl matches the two by name. A host the manifest names no scroll
for is skipped, never POSTed to, and not counted against the exit code.

This is why `fleet` needs no configuration file. The name is the join key
between three otherwise-independent artifacts — the VM, the inventory, and the
manifest — and everything else falls out of it.
