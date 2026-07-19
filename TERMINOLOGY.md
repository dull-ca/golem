# Golem — terminology

## The boundary

- **Blueprint** — the thing a user commissions: a named, self-contained
  description of a set of Hosts and the Workloads, Services, and Ingress that
  run on them — a complete system golem builds and keeps running. Each
  Blueprint is sealed off from every other: its members reach one another over
  a private internal network (which may span several subnets and hosts), and
  nothing outside gets in except through an Ingress — a seal that holds even
  when two Blueprints share a host. Re-commissioning the same name replaces it.

## What a Blueprint declares

A Blueprint declares Hosts; everything else hangs off a Host.

- **Host** — a machine things run on, and the container for everything that
  runs on it. A Host's attributes include the Workloads, Services, and Ingress
  placed on it; placement is containment — a thing runs on the Host that
  declares it.
- **Workload** — a container that runs but is not attached to any network.
- **Service** — a container that runs and is on the blueprint-internal network
  (internal only — reachable from elsewhere in the same Blueprint, not from
  outside).
- **Ingress** — a sanctioned hole in the boundary, declared on the Host that
  serves as the entry point: how traffic is allowed into the Blueprint, from
  the outside world or from inside it.

## Lifecycle

- **commission / decommission** — what a user does to a Blueprint. Commission =
  request it be present (add or replace); decommission = request it be gone.
- **build / teardown** — what golem does in answer to a commission /
  decommission: the realization of the Blueprint, performed by the builder.
- **Action** — a single recorded step within a build/teardown (e.g. building a
  Service, tearing down an Ingress).
- **State** — the resolved view of what a golem is meant to run on its host(s):
  the Workloads, Services, and Ingress currently called for, and which
  Blueprint(s) call for each.
- **Revision** — an append-only journal entry, one per change (Init /
  Commission / Decommission), embedding the State and Actions at that point.

## A golem's anatomy

- **golem** — one running instance on one Host. Self-directing: given the
  agreed plans, it builds its own host's slice with no execution-time
  coordination with anyone else. Made of a foreman and a builder.
- **foreman** — the brain. Holds the Blueprints, resolves what belongs on this
  host, decides what to build / teardown, directs the builder, and keeps the
  journal (Revisions). Takes its plans from the plan room.
- **builder** — the hands. The one concrete implementation of how things get
  done on a real platform (e.g. Debian trixie via apt / nginx / nftables /
  quadlet). Compiled into the golem; one per golem; every golem in a caucus
  runs the same builder and version, so one caucus = one platform.
- **rolodex** — the family of possible builders (trixie-builder, compose-builder,
  …); a way to refer to the set.

## Across golems

- **caucus** — the group of golems that share a common set of Blueprints.
- **consensus** — the process (Raft) by which the caucus agrees: a leader is
  elected, a log of changes is replicated, an entry commits once a quorum has
  it.
- **quorum** — the majority needed to ratify (elect a leader, commit an entry);
  the bar consensus must clear.
- **plan room** — a golem's local store of the Blueprints it holds and builds
  from.
