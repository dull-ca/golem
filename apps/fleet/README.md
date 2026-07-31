# fleet

An ephemeral fleet of local Debian VMs for exercising golem's real reconcilers.

golem's unit tests drive the reconcilers through a fake host; that proves the
logic but never touches apt, systemd, or a real filesystem. `fleet` closes that
gap. It boots throwaway rootless-QEMU Debian-trixie VMs, deploys a static
`golemd` to each, applies a scroll, and lets you read the journal and logs —
`golemd` running its `host` reconciler against an actual box, discovering the
things a fake host cannot (the ADR-0015 `daemon-reload`-before-enable fix was
found this way).

Everything is ephemeral: a VM is a copy-on-write overlay on one shared base
image, and `reset` wipes the lot back to a clean slate.

## The base image

Guests boot Debian's `genericcloud` qcow2 from `trixie/latest`. genericcloud
ships cloud-init and no cloud-vendor agents, so a plain read-only seed ISO —
hostname, an ssh key, a passwordless sudoer — is enough to bring a guest up
unattended, with no per-provider configuration. The newest image is discovered
and downloaded once, then cached under `.fleet/images/` and reused by every VM.

## Quickstart

Run everything through the `fleet` devenv script (it puts you at the repo root
with the harness importable):

```bash
fleet up                                  # boot the six lichess VMs
fleet deploy                              # build + install golemd on each
fleet apply examples/lichess/fleet.emet   # compile the scroll, POST to each daemon
fleet status                              # who is up, golemd reachable, last revision
fleet logs scaly -f                       # tail one guest's golemd journal
fleet reset                               # kill everything, back to a clean slate
```

`up` takes `--hosts a,b` for named VMs or `--count N` for the first N lichess
hosts. `deploy`, `apply`, and `logs` take `--hosts` to target a subset; without
it they hit every VM. `apply` accepts either an `.emet` source (compiled here)
or a prebuilt `manifest.bin`; relative paths anchor at the repo root.

On a guest, `journalctl -u <unit>` works as the `golem` user without sudo — the
cloud-init user data adds it to the `systemd-journal` group. Existing VMs booted
before this change need a `fleet reset` (cloud-init runs once per instance) or a
one-off `sudo usermod -aG systemd-journal golem` to pick up the membership.

## Port scheme

Each VM claims a slot derived from its **name**, not its position in the boot
list: `slot = blake2b(name) mod 100`, so a given name always lands on the same
ports no matter what else is booted, and booting one host alone gets the same
ports it would in the full set. ssh forwards to `2200+slot`, golemd to
`8800+slot`:

| name     | slot | ssh (host → guest 22) | golemd (host → guest 7474) |
|----------|------|-----------------------|----------------------------|
| registry | 65   | 2265                  | 8865                       |
| builder  | 3    | 2203                  | 8803                       |
| puller   | 68   | 2268                  | 8868                       |

golemd listens on `0.0.0.0:7474` inside the guest; QEMU forwards `8800+slot` on
localhost to it, so the CLI reaches each daemon over plain HTTP.

The slots are collision-free across the default lichess host set plus the
`registry`/`builder`/`puller` dogfood names. Ports for an already-created VM are
read from its `state.json` record, so VMs booted under the old positional scheme
keep the ports they were assigned — the name→slot map only governs a fresh boot.

## Extra port forwards and cross-VM traffic

Each guest runs behind user-mode (SLIRP) networking and is isolated from the
other guests: only ssh and golemd are forwarded to the host. Two facts make
one guest reach a service on another:

- `up --publish` forwards an extra guest port to the host. `--publish
  registry=5000:5000` binds host `127.0.0.1:5000` to the `registry` guest's
  `:5000`; a bare `--publish 5000:5000` publishes on every booted host (which
  clashes if they share a host port, so name the host for a single service).
  The forwards are recorded per VM, so a stopped VM brought back up re-forwards
  the same ports.
- In SLIRP every guest reaches the host at `10.0.2.2`. So a guest reaches
  another guest's *published* port at `10.0.2.2:<host_port>`: the connection
  lands on the host's loopback, which QEMU forwards into the publishing guest.

Together these give a host-gateway rendezvous. `examples/registry/` uses it:
the `registry` guest publishes its `:5000`, and the `builder`/`puller` guests
push and pull from `10.0.2.2:5000` — one golem-hosted registry shared across
machines, no shared L2 segment required.

## Ephemeral state

Everything the harness writes lives under `.fleet/` at the repo root:

- `images/` — the cached base image.
- `id_ed25519[.pub]` — the fleet keypair, injected into every guest.
- `state.json` — the record of which VMs exist (ports, pid, disk paths).
- `vm-<name>/` — one per guest: its overlay disk, cloud-init seed, pidfile, and
  serial-console log.

`down` stops a VM's qemu process but keeps its overlay disk, seed, and state
record. A later `up` on that name **resumes** it — re-launching qemu against the
existing disk on its recorded ports — so guest data written before `down`
survives. Only `reset` wipes: it kills every VM and deletes all per-VM data and
the state file — all guest data is lost — but keeps the cached image and keypair
for a fast next `up`. `reset --purge` drops those too, so the next `up`
re-downloads the image and regenerates the key.

A guest keeps only data it has flushed to disk. `down` sends qemu SIGTERM, which
does not sync the guest's page cache, so a write made moments before `down` can
be lost across the cycle; run `sync` in the guest before `down` if a very recent
write must survive.

## Smoke fixtures

Three small scrolls to sanity-check a fresh box:

- `smoke.emet` — one of each host-touching glyph (`aptPackage`, `file`,
  `lineInFile`), the minimum that a reconcile actually changed the host.
- `reload-proof.emet` — writes a systemd unit file with a `file` glyph, then a
  `systemdService` for it. Proves the `daemon-reload`-before-enable fix: without
  the reload, enabling a just-written unit fails.
- `notify-proof.emet` — a `service` leaf holding the unit, and a sibling `config`
  leaf that only `notifies` it. The config lives outside every unit directory, so
  the structural heuristic cannot see it: the unit is reached solely through the
  authored notification (ADR 0036). Edit the config, re-apply, and
  `journalctl -u golem-notify.service` should show the reload.
