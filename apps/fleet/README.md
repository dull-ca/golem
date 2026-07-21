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

## Port scheme

Each VM claims a slot `i` by its position in the boot list and takes the same
offset on every base:

| slot | ssh (host → guest 22) | golemd (host → guest 7474) |
|------|-----------------------|----------------------------|
| 0    | 2200                  | 8800                       |
| 1    | 2201                  | 8801                       |
| …    | 2200+i                | 8800+i                     |

golemd listens on `0.0.0.0:7474` inside the guest; QEMU forwards `8800+i` on
localhost to it, so the CLI reaches each daemon over plain HTTP.

## Ephemeral state

Everything the harness writes lives under `.fleet/` at the repo root:

- `images/` — the cached base image.
- `id_ed25519[.pub]` — the fleet keypair, injected into every guest.
- `state.json` — the record of which VMs exist (ports, pid, disk paths).
- `vm-<name>/` — one per guest: its overlay disk, cloud-init seed, pidfile, and
  serial-console log.

`down` stops a VM but leaves its disk and state, so it can be brought back up.
`reset` kills every VM and deletes all per-VM data and the state file — all
guest data is lost — but keeps the cached image and keypair for a fast next
`up`. `reset --purge` drops those too, so the next `up` re-downloads the image
and regenerates the key.

## Smoke fixtures

Two small scrolls to sanity-check a fresh box:

- `smoke.emet` — one of each host-touching glyph (`aptPackage`, `file`,
  `lineInFile`), the minimum that a reconcile actually changed the host.
- `reload-proof.emet` — writes a systemd unit file with a `file` glyph, then a
  `systemdService` for it. Proves the `daemon-reload`-before-enable fix: without
  the reload, enabling a just-written unit fails.
