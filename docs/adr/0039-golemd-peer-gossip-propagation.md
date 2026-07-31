# 0039 — golemd peer gossip: manifest propagation and fleet plan wave

## Status

Proposed 2026-07-31. Phase 2 of fleet orchestration; phase 1 (client-side
fan-out, ADR 0038) is the accepted first step and stays useful alongside
this. Unbuilt — this record holds the design so the open question is
logged, not lost.

## Context

ADR 0038's fan-out requires the operator's machine to reach every daemon.
The requested end state is stronger: networked golems that pass along every
new scroll definition they receive, so submitting a manifest to *one*
daemon reaches the whole fleet — including hosts the operator cannot dial
directly. Forces: the manifest is already content-addressed (BLAKE3 over
the bytes), so "new to me" has a natural key; `POST /manifest` is
fire-then-poll (ADR 0033) and must keep its shape; `POST /plan` is
read-only and its fleet variant must aggregate results synchronously;
there is no signing yet (QUICKSTART "what this isn't"), so gossip widens
the unauthenticated surface from "whoever reaches a port" to "whoever
reaches any port"; the local VM fleet has full pairwise reachability via
the host-gateway rendezvous (`10.0.2.2:<forwarded port>`).

## Decision (proposed)

golemd learns a static peer set — `[fleet] peers = ["http://…", …]` in
`golemd.toml` plus a repeatable `--peer` flag — and two propagation
behaviors sharing one seen-cache module:

- **Apply gossip, dedup by content.** After a `POST /manifest` ingest
  (success or failure), the daemon forwards the raw manifest bytes to
  every peer unless the manifest's BLAKE3 hash is already in a TTL'd
  seen-cache (in-memory, ~10 min). Receivers do the same, so the flood
  covers any connected topology and terminates: each node ingests a given
  manifest at most once per TTL window. A deduplicated re-receipt answers
  202 with the reconcile id the first receipt produced, keeping the
  poll contract. A 409-busy local ingest retries briefly (bounded, logged)
  so one in-flight reconcile doesn't drop a node from the flood; forward
  failures are logged and exposed, not queued.
- **Plan wave, dedup by request.** `POST /fleet/plan` carries an
  `X-Golem-Request-Id`; first sight computes the local plan, forwards to
  all peers, and returns its report merged with theirs (deduped by host,
  bounded by a peer timeout); repeat sight returns empty. Content-hash
  dedup is wrong here — planning the same manifest twice in a row is
  routine — so the wave is scoped per request instead.
- **Observability.** `GET /peers` lists the configured peers with each
  one's last forward outcome.

The VM harness renders each guest's peer list (all other guests at
`10.0.2.2:<port>`) into the deployed `golemd.toml`.

**Precondition — absence is silence.** golemd today resolves a host
absent from a manifest to the empty scroll (`foreman::select`), i.e.
absence is a removal order. Under gossip every manifest reaches every
peer, so that fallback would let any partial manifest wipe every host it
doesn't name. Before propagation ships, golemd must adopt the fleet rule
ADR 0038 already applies client-side: a manifest without my scroll makes
no statement about me — the daemon acknowledges without opening an
attempt; decommission requires an explicitly present empty scroll.

## Consequences

- Submit-to-one reaches all, over exactly the transport and formats that
  exist; no membership protocol, no persistent queues, no anti-entropy —
  a down peer misses the flood and catches up only on the next submit.
  Those are the first refinements if the static-peer flood proves too
  weak.
- The unauthenticated apply surface becomes transitive. Per ADR 0040
  golem itself will not close that: the infra layer must (UDS, private
  segments, tunnels), and a gossip mesh must not span a trust boundary
  the infra hasn't secured.
- Two dedup keys (content hash for apply, request id for plan) are the
  cost of matching each verb's semantics; the shared cache keeps it one
  mechanism.
- golemd grows an HTTP client and background forwarding tasks — its first
  outbound behavior. The reconcile core is untouched; propagation sits
  entirely in the HTTP adapter layer.
