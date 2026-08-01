# 0038 — Fleet-wide apply/plan: golemctl fan-out over a TOML inventory

## Status

Accepted 2026-07-31. Implementation plan:
`docs/superpowers/plans/2026-07-31-fleet-fanout.md`. The daemon-side
alternative — golemd-to-golemd manifest propagation — is logged as ADR 0039
(Proposed) and is the intended follow-on, not a rejected path.

## Context

A manifest already carries every host's scroll, but nothing fans it out:
`golemctl apply` targets one daemon, and the `fleet` VM harness loops
per-host on the operator's machine — a harness convenience, not a product
surface. Applying a change to the whole fleet means N invocations against N
addresses. Two shapes can close the gap: the daemons propagate manifests to
each other (peer gossip, new wire surface, peer config, dedup), or the CLI
fans out to a declared set of endpoints (ansible's shape: an inventory of
connections, one command, per-host results). Constraints: golemctl already
owns the whole single-host protocol — compile, `POST /manifest`, poll,
fold, live unit tree (ADR 0033) — and each daemon independently selects its
own scroll from the shared manifest, so fan-out needs no daemon or wire
change at all. One trap: golemd resolves a host absent from the manifest
to the empty scroll, so naively fanning a manifest out to every inventory
host would decommission any host the manifest doesn't name. A fleet
submit must treat absence as silence, not as a removal order.

## Decision

Fleet orchestration starts client-side, in golemctl. A TOML **inventory**
(`[hosts]` table, name → golemd base URL; resolved `--inventory` flag, then
`$GOLEMCTL_INVENTORY`, then `./fleet.toml`) declares the connections. A new
`fleet` subcommand group fans the existing verbs out concurrently:

- `golemctl fleet apply <source>` — compile once, fire every host's
  `POST /manifest` concurrently, poll each reconcile to its terminal
  phase; on a TTY, one live tree with a branch per host (nested spinners:
  host branch animates while its units enact), each branch the existing
  unit tree; otherwise host-prefixed plain lines and, with `--json`, a
  final `{"hosts": {name: report | error}}` object. Exit 0 iff every host
  settled; one host's transport failure or 409 never stops the others.
- `golemctl fleet plan <source>` — concurrent `POST /plan`, each host's
  report rendered with the existing plan view under a host heading;
  `--json` returns the aggregate verbatim.
- `golemctl fleet status` — one line per inventory host: reachable,
  daemon host id, latest revision, applied content id.

`fleet apply` and `fleet plan` decode the manifest once and target only
the inventory hosts whose names it contains: a host in the inventory but
not in the manifest is **skipped** — reported distinctly, never POSTed
to, never affecting the exit code. Decommissioning a host therefore
requires an explicitly authored empty scroll for it; absence makes no
statement. (Single-host `golemctl apply` keeps its documented semantics —
directly targeting one daemon is an explicit order.)

`--hosts a,b` filters the inventory subset on any fleet verb; an unknown
name is an error naming the known set. The VM harness gains
`fleet inventory`, which renders `.fleet/inventory.toml` from its state
file so the local fleet is drivable by the new verbs unchanged.

## Consequences

- One command applies a set of scrolls to every machine at once, with
  per-host isolation of failure and a fleet-level live view — the ansible
  shape, delivered without touching golemd, the wire format, or trust
  assumptions (the operator's machine still reaches every daemon
  directly).
- The inventory is a second fleet description beside the Emet program:
  Emet names the scrolls, the inventory names the endpoints. They meet
  only at the host name; drift between them surfaces as reported skips
  (or as unreachable endpoints), never as surprise removals.
- Fan-out requires the operator's machine to reach every daemon — no
  relay through a reachable peer. True golem-to-golem propagation (submit
  to one, all receive) stays open as ADR 0039; this ADR neither builds
  nor forecloses it, and the inventory verbs remain useful alongside it
  as the direct-drive surface.
- Concurrent applies make overlapping reconciles ordinary: a daemon busy
  with a prior attempt answers 409, reported per-host rather than
  retried. Queueing or reattach-on-conflict is future work if it bites.
