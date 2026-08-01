# 0040 — Trust and transport security live in the infrastructure, not in golem

## Status

Accepted 2026-07-31 (decision by Dr. Dub). Bears on ADR 0038 (fleet
fan-out) and ADR 0039 (peer gossip); revises 0039's "signing before
gossip" consequence. Refined by ADR 0042: transport stays infra's, but
golemd gains a single shared-secret authorization check.

## Context

golemd's HTTP surface is unauthenticated: whoever reaches the port can
apply a manifest. As fleet surfaces grow (fan-out today, peer gossip
next), the reachable surface grows with them, and the obvious reflex is to
teach golem signing, tokens, or TLS identity. Each of those complicates
the daemon, the wire format, and key handling — machinery that mature
infra layers already provide.

## Decision

golem does not enforce a security model. Confidentiality, authenticity,
and reachability of the golemd surface are the fleet/infra layer's
responsibility, composed from standard mechanisms: unix domain sockets
with filesystem permissions, loopback-only binds, network segmentation
and firewalling, ssh tunnels or mesh VPNs where links cross trust
boundaries. golem's own fleet abstractions may *provision* such
mechanisms (they compile to the four glyph kinds like anything else), but
golemd and golemctl stay credential-free and verify nothing about a
caller beyond the transport delivering it.

## Consequences

- The daemon and wire format stay simple; no key distribution, no auth
  negotiation, no format bump for identity.
- Deployment bears the burden: an operator who binds golemd to a routable
  interface has published root-equivalent control of that host. Docs must
  say so plainly wherever `--listen` is mentioned.
- Connection shapes beyond `http://host:port` (a UDS path first of all)
  become inventory/config concerns; the per-host table form in the
  inventory (ADR 0038) is the extension point.
- ADR 0039's gossip is unblocked from any signing prerequisite: peers are
  trusted because the network says so, and a gossip mesh must simply not
  span a trust boundary the infra hasn't secured.
