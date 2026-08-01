# 0042 — SSH-only transport, loopback golemd, shared-secret authorization

## Status

Accepted 2026-07-31 (decision by Dr. Dub). Refines ADR 0040: transport and
network reachability remain the infrastructure's concern, but golem gains a
minimal application-layer authorization gate. ADR 0040's "golemd and
golemctl stay credential-free and verify nothing about a caller" is
superseded on the golemd side by exactly one check; everything else there
stands.

## Context

golemd's port is root-equivalent control of its host, and the fleet verbs
(ADR 0038) multiply how often it is dialed. Golem must never be exposed to
the public internet, and even inside the perimeter only a select set of
people may submit changes. Per ADR 0040 golem does not build a security
stack — but "reachable port implies authorized" stops being acceptable the
moment a second person can reach the box. Forces: every production host
already has SSH as its sole ingress; operators already hold working SSH
connections (agents, ControlMaster); lichess (the first user) runs
authentik with lichess SSO, but a full OIDC flow in golemctl is machinery
ahead of need; a shared secret in the team's shared path, surfaced to
golemctl through the environment, is the shape Dr. Dub prefers first. The
local VM fleet must work exactly like production or it stops being a
rehearsal.

## Decision

Three layers, each owned by the party ADR 0040 assigns:

1. **SSH is the only way onto a box.** golemd binds loopback
   (`--listen 127.0.0.1:7474`); no golemd port is ever forwarded or
   routable. The VM harness drops its direct golemd port-forwards' use and
   deploys loopback-bound daemons.
2. **golemctl rides the existing SSH connection.** A target may be
   `ssh://user@host[:port]` (or inventory fields `ssh`/`ssh_port`/
   `remote_port`/`ssh_args`): golemctl opens a local forward to the
   daemon's loopback port over ssh — reusing a ControlMaster when the
   operator's config provides one — and speaks plain HTTP through it. No
   TLS in golem; the tunnel is the encryption and the host authentication.
3. **A shared secret authorizes, even then.** golemd reads a token file
   (`[auth] token_file` / `--auth-token-file`) and requires
   `Authorization: Bearer <token>` on every request, compared in constant
   time; failures are a typed 401. golemctl sources the token from
   `GOLEM_AUTH_TOKEN` or `GOLEM_AUTH_TOKEN_FILE` (per-host inventory
   `token_file` overriding). No token file configured = the gate is off —
   the dev/test posture, never the deployed one.

Bearer-over-tunnel rather than request signing: an HMAC scheme defends
against on-path replay, and inside an SSH tunnel there is no on-path.
Authentik/OIDC stays the recorded evolution for lichess SSO — the gate is
one header check, so swapping "compare a secret" for "verify a token
issued by authentik" changes the middleware, not the architecture.

## Consequences

- Submitting changes requires membership in two sets at once: people who
  can SSH to the box, and people who hold the shared secret. Revocation is
  rotating one file.
- The secret lives in the team's shared path and reaches golemctl by
  environment; it crosses the wire only inside SSH. Its file on the host
  is root-owned, mode 0600.
- Every golemctl round-trip gains SSH tunnel setup unless a ControlMaster
  is already up; with one, the forward is milliseconds. The fleet verbs
  open one tunnel per host per invocation.
- The VM fleet's direct golemd forwards (`88xx`) go dead by construction —
  the guest daemon no longer listens on a forwardable address. The harness
  provisions the token, deploys loopback daemons, and emits an ssh-form
  inventory, so `golemctl fleet …` against the VMs rehearses production
  exactly.
- golemd's gate is deliberately one check. Per-user identity, audit trails
  of who submitted, and SSO group mapping all arrive with the authentik
  follow-on, not before.
