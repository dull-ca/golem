# 0041 — Composed host config: one drop-in file per glyph, never shared-file line composition

## Status

Accepted 2026-07-31 (decision by Dr. Dub). Bears on the Emet library layer
(ADR 0023's abstractions); no golemd or wire change. First applied to the
lichess example's nftables story.

## Context

The lichess `ingress` abstraction composed `/etc/nftables.d/ingress.nft` by
appending one `lineInFile` per ingress into a shared file. Nothing loaded
the file, so it was harmless — but as a pattern, shared-file composition
gives emergent content: no single owner, ordering decided by enact order
rather than declaration (wrong the moment order-sensitive rules join),
dedup left to luck, and cleanup meaning line surgery in a file other units
still depend on. The alternative shapes: one glyph owning the whole merged
file (a fold in Emet — deterministic, but every contributor's change
re-renders one big glyph), or one file per contributing glyph in a drop-in
directory, merged by the consumer. nftables natively supports the latter
(`table`/`chain` blocks are additive across an atomic `nft -f` load), as do
most Unix config consumers (`conf.d`, `sites-enabled`). One hazard framed
the design: a naive `include` from the distro's `nftables.conf` inherits
its `flush ruleset`, which would wipe tables other tools (podman/netavark)
own on every reload.

## Decision

Config that golem composes from several units is authored as **one
complete drop-in file per glyph**, in a directory consumed by an
entrypoint golem also owns — never by `lineInFile` into a shared file.
`lineInFile` is reserved for editing files golem does *not* own. Ordering,
where it matters, is carried by file names (a `00-` base sorts first);
dedup of identical contributions is ADR 0034's content-id credit, not
string comparison.

For nftables specifically: golem owns `table inet golem` and nothing else.
A base drop-in declares the hooked input chain (policy drop, with
established/related, loopback, icmp, ssh, and golemd accepts so a fleet
box stays reachable); each ingress or internally-exposed service
contributes its own additive drop-in. The entrypoint conf is
`add table inet golem` + `flush table inet golem` + `include` of the
drop-in glob — idempotent reloads that never touch another tool's tables —
loaded by a golem-owned oneshot `golem-nftables.service` (`ExecReload`
re-runs the load; `ExecStop` deletes the table), reloaded via unit-level
`notifies` (ADR 0036).

## Consequences

- Cleanup is exact and local: removing a unit reverses its files, the
  notify reload re-derives the ruleset from the files that remain, and
  removing everything stops the service and with it the whole table. No
  line surgery, no orphaned fragments.
- Duplicate contributions (two ingresses both accepting 443) become two
  benign rules rather than a coordination problem; nftables' first-match
  makes them redundant, not wrong.
- Policy drop makes the base allowlist load-bearing: any host-inbound
  port a program does not declare is now dropped. That is the feature —
  but abstractions must declare what they listen on, and the base must
  keep ssh and golemd reachable or a fleet box orphans itself.
- Golem never edits a distro-owned nftables file, so the example's only
  `lineInFile` use disappears; the glyph kind remains for genuinely
  foreign files (`smoke.emet` still exercises it).
