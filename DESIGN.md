# Golem — Design

A small-fleet declarative orchestrator for bare-metal Debian boxes that is honest about pre-existing state.

This document is the source of truth for *what Golem is, who it's for, and how it's built*. It supersedes the speculative framing in the original scaffold's README. The 7 design commitments survive; the layering model is sharpened.

---

## 1. Audience and goal

**Audience:** small-fleet operators who run bare metal and reject the "cloud is the only answer" narrative. Concretely:

- **Lichess-shaped:** ~20–30 OVH/Hetzner boxes, ~100 services, single operator, services move between boxes constantly. Currently hand-bombed; cleanup is a real and recurring pain.
- **Strabs-shaped (and the local-non-profit / OSS-project tail):** a handful of independent clients sharing a $100/mo bare-metal box (Webfaction-as-coop), each running a few containers with SSL and a database. Currently each pays $20–40/mo for an isolated VPS.
- **DHH-cloud-exit category:** teams that have read the 37signals posts, looked at their AWS bill, and want a sane way to run on owned hardware without re-implementing Kubernetes.

**Goal:** be the best honest-cleanup orchestrator for bare-metal Debian, sustainable as a solo-maintained OSS project, with a modest managed-control-plane offering that covers infra costs.

**Explicit non-goals:**
- Multi-tenant SaaS dashboard, SSO, billing.
- Full-time business or VC-scale outcome. Strabs switching alone covers infra cost; that's the success bar.
- Replacing Kubernetes. If you have >50 boxes or need rolling deploys with traffic shaping, use k3s.
- Windows, macOS, non-Debian Linux. Debian trixie is the target.

---

## 2. The three-layer model

Golem has three layers, and they must not be conflated:

| | Layer | Audience | Shape | Lifetime |
|---|---|---|---|---|
| 1 | **Input language** (Nickel) | tenants + operators | `Inventory` of `Hosts`, each carrying `workloads` (containers) + `services` (port routing with `allow_from`) + `ingress` + `tags`. Cross-cutting `groups` and `role_deps` at the Inventory level. Statically verifiable, few lines per workload. | written by humans |
| 2 | **Translation** (Nickel eval, operator-side, pre-sign) | Golem authors | For each host, expands each Workload → Quadlet + SystemdUnit, each Service → NftFragment for the firewall, each Ingress → nginx site + (optionally) DNS record. Resolves `g.ref.host` / `g.ref.service` placeholders. Produces a signed bundle of layer-3 primitives per host. | runs on operator's laptop |
| 3 | **System primitives** (wire format + agent state engine) | agent only | `AptPackage`, `SystemdUnit`, `File`, `Quadlet`, `NftFragment`, `DnsRecord`, plus their reconciliation machinery | persisted on each host |

**The rule:** users only see layer 1. The agent only sees layer 3. Layer 2 is a Golem implementation detail that runs *before* signing, on the operator's laptop. The wire format is layer 3.

This is not a high-level / low-level *user* split. It is a translation pipeline. Tenants do not "drop down" to layer 3. Lichess's "manage one weird file alongside hand-bombed stuff" still goes through layer 1 — there's a small `system` escape hatch on a Host for raw `packages` / `files` / `units` that translate 1:1 into layer-3 claims when the high-level shapes don't fit.

---

## 3. Layer 1 — input language

Nickel. The shape is borrowed in spirit from the lichess Ansible
inventory DSL but with the lichess-specific shapes (Mongo, Database,
Fishnet, JavaConfig) collapsed back into the two general primitives
that subsume them: `Workload` (the container) and `Service` (port
routing with a firewall). The schema is small on purpose.

### `Inventory` — every host, what runs there

```nickel
let lila = {
  image = "ghcr.io/lichess-org/lila:latest",
  env   = { LILA_DB_URI = "mongodb://%{g.ref.service "mongo-lichess"}/lichess" },
} in

let mongo = fun instance => {
  name   = "mongo-%{instance}",
  image  = "mongo:7",
  volume = { at = "/data/db", size = "200Gb" },
} in

{
  hosts = {
    "edge-01" = {
      name      = "edge-01",
      public_ip = "203.0.113.10",
      ingress = [
        { host = "lichess.example.com", backend = "lila", dns = true,
          rate_limit = "100r/m", rate_limit_burst = 20 },
      ],
    },

    "app-01" = {
      name      = "app-01",
      public_ip = "203.0.113.20",
      vrack_ip  = "10.42.0.2",
      tags      = ["mongodb-clients"],
      workloads = [ lila & { name = "lila" } ],
      services  = [ { name = "lila", workload = "lila", port = 9663,
                      allow_from = ["edge-01"] } ],
    },

    "db-01" = {
      name      = "db-01",
      vrack_ip  = "10.42.0.3",
      workloads = [ mongo "lichess" ],
      services  = [ { name = "mongo-lichess", workload = "mongo-lichess",
                      port = 27017, allow_from = ["mongodb-clients"] } ],
    },
  },
} | g.Inventory
```

Each Host can carry:

- **`workloads`** — the containers that run on this host. Image, env,
  volumes, command. The schema doesn't distinguish "stateful" from
  "stateless", "data" from "app" — that's the user's pattern in their
  templates.
- **`services`** — internal port routing. A name + `(workload, port)` +
  an `allow_from` firewall surface. Multiple Services can target one
  Workload (different ports → different `allow_from`); pure-outbound
  workloads have zero Services.
- **`ingress`** — public HTTPS termination (nginx) with rate limits,
  connection limits, CORS, routes, and a `dns` flag that makes golem
  own the A record. `backend` names a Service.
- **`tags`** — host membership in cross-cutting groups for `allow_from`.

There's no `version` field. `golemctl apply` queries each host's
`/status` to read its last-applied version and bumps automatically —
version numbers are bookkeeping golem owns, not something operators
type.

### Firewall: `allow_from` is the rule

`allow_from` on a `Service` is a list of hostnames or group names that
may connect to its port. The translator emits an `NftFragment` claim
on the host the workload runs on. Default is **deny everything except
localhost** — opt-in, not opt-out.

Group names come from `Inventory.groups` (explicit list) and from host
`tags` (a host with tag `"mongodb-clients"` is a member of group
`"mongodb-clients"` automatically). Misspell a host or group name and
`golemctl apply` rejects.

### DNS: `dns = true` on Ingress

When an Ingress has `dns = true`, golem owns the DNS record for its
`host`. The record is a layer-3 `DnsRecord` claim emitted on whichever
host carries the DNS role for the zone (see `Inventory.role_deps`).
Removing the Ingress removes the record honestly — no separate
DNS-provisioning tool to drift out of sync.

### Cross-host references

Workloads don't hardcode addresses. They use placeholders:

- `{name.service}` — host:port for the named Service, anywhere in the
  inventory. The translator finds the host carrying the Service and
  resolves to that host's private address + the Service's port.
- `{name.host}` — primary address of the named host (vrack_ip if set,
  else public_ip). For raw addressing without a port.
- `g.ref.service "name"` / `g.ref.host "name"` — Nickel helpers that
  emit the same placeholders but check existence at translate time.
  Misspelled names are rejected with a clear error, not surfaced as
  `getaddrinfo` failures at 3am.
- `g.ref.secret "name"` — placeholder for an operator-supplied secret.
  Values stage on the operator's laptop under
  `~/.golem/secrets/<inventory>/`, encrypted into the bundle, decrypted
  to a tmpfs on the agent. (M3.)

You never type a port mapping or a container name. "The mongo-lichess
service" is how you refer to mongo on db-01.

### Templates and record merge

A `Workload` is just a record. Write the common shape once, instantiate
per host with `&`:

```nickel
let lila = {
  image = "ghcr.io/lichess-org/lila:latest",
  env   = { LILA_DB_URI = "mongodb://%{g.ref.service "mongo-lichess"}/lichess" },
} in

workloads = [
  lila & { name = "lila"        },
  lila & { name = "lila-canary",
           env.FEATURE_FLAGS = "canary" },
]
```

`&` is Nickel's record merge — right-hand fields win. There's no
inheritance, no class hierarchy, no template language. Just records and
merge. This is how the lichess inventory's repeated-with-variation
pattern collapses into a few templates and a lot of merges. "Mongo",
"Postgres", "Fishnet", "Lila" are user-land patterns over `Workload`;
the schema knows nothing about them.

### Trust tiers

The narrow harness only works if the harness *is* narrow:

- **Tenant-trust** — `workloads` and `services` on a Host. A team
  running an app can declare containers, env, volumes, references
  between them, and the firewall surface for the ports they expose.
  Nothing here reaches the host directly; every workload compiles to
  a quadlet + a unit, and every service compiles to an nft fragment.
- **Operator-trust** — Host addresses, ingress, DNS, role_deps, and any
  raw `system.*` escape-hatch claims. The operator's signing key is
  what authorizes them.

A strabs tenant who writes a `system.file` for `/etc/sudoers` gets a
Nickel contract violation at `golemctl apply` time, not a CVE.

### Why Nickel

The constrained surface is the harness. Same reason a tightly-prompted LLM produces better code than a maximally-flexible one: fewer footguns, faster mental model, battle-tested primitives composed predictably. Nickel adds gradual contracts (static-ish typing on config), merge semantics for environment overrides, and clean record syntax. The 8 lines of Nickel a tenant learns are paid back the first time a config typo is caught before it hits a node.

### What layer 1 is *not*

- It is not the wire format. The signed bundle does not contain `app.*` or `expose.*` — those are translated away.
- It is not extensible at runtime. Adding a new layer-1 primitive (a new field on `Service`, a new `expose` shape) means a Golem release with a new translator. This is intentional — the harness's value is its narrowness.
- It is not a programming language. Nickel is config; logic belongs in services.

---

## 4. Layer 2 — translation

The translator runs as part of `golemctl apply` on the operator's laptop. It evaluates the Nickel module for each target node, then expands layer-1 primitives into layer-3 claims, then signs the result.

### Expansion rules

| Layer 1 | Translates to (Layer 3) |
|---|---|
| `Workload` | A `Quadlet` for `/etc/containers/systemd/<name>.container`. A `SystemdUnit` for `<name>.service` (the auto-generated quadlet unit), enabled + active. A handler so the unit restarts when the quadlet body changes. The first workload on a host triggers an `AptPackage` claim for `podman` (idempotent across workloads). |
| `Workload.volume` / `volumes` | A `File` for the podman-volume definition (`<name>-<vol>.volume` Quadlet). The container's `Volume=` line references it. |
| `Workload.configs[]` | A `File` at the declared path; the container quadlet bind-mounts it in. |
| `Service` | An `NftFragment` claim materializing the firewall rule: accept `port` from each listed host or group's addresses, drop otherwise. The Service's `name` is the routing identity for `g.ref.service` and ingress `backend` references. |
| `Ingress` (one per ingress entry) | A `File` for `/etc/nginx/sites-enabled/<host>.conf` carrying rate-limit zones, conn-limit zones, locations, and routes. The `backend` Service is resolved to host:port at translate time. An `AptPackage` claim for `nginx` (idempotent across ingresses). A `SystemdUnit` for `nginx.service` enabled + active. A handler to reload nginx on file changes. An `NftFragment` for 80/443. |
| `Ingress.dns = true` | A `DnsRecord` claim (kind A or AAAA) emitted on the host carrying the DNS role for the zone, pointing the ingress hostname at the ingress host's public addresses. |
| `g.ref.service "name"` placeholder | Resolved at translate time to `<host>:<port>` for the Service named `name`. The translator looks up which host owns the Service. |
| `g.ref.host "name"` placeholder | Resolved at translate time to that host's `vrack_ip` if set, else `public_ip`. |
| `g.ref.secret "name"` | A `File` at `/run/golem/secrets/<inventory>/<name>` (tmpfs, mode 0600). Container env or mount references resolve to that path. |
| `system.files."<key>"` (per-host escape hatch) | A `File` claim, 1:1. |
| `system.packages."<name>"` | An `AptPackage` claim, 1:1. |
| `system.units."<name>"` | A `SystemdUnit` claim, 1:1. |

Quadlet is podman's native systemd-integration mechanism (drop a `.container` file in `/etc/containers/systemd/`, `daemon-reload`, get a generated `.service` you can start). The agent has a `Quadlet` claim kind in addition to `File` so the daemon-reload-after-container-file handler is implicit, not something every Service translation has to wire up.

### Why translate operator-side, not on the agent

- **The agent stays small.** Three providers (File, AptPackage, SystemdUnit) plus the engine. No Container or Ingress logic, no Caddy template knowledge, no podman version sniffing.
- **Static verification happens before bytes leave the laptop.** Typos and contract violations fail at `nickel export`, not at reconcile time on a remote node.
- **The wire format is versioned, not "stable forever."** Layer-1 evolves with Golem releases; layer-3 evolves more slowly but it *will* evolve (per-claim timeouts, capture-size cap, future provider variants). The bundle envelope carries an explicit `layer3_version`; the agent rejects bundles whose layer-3 version it doesn't understand and surfaces the version mismatch in `/status`. Compatibility window: agents accept the current layer-3 version and the previous one; older bundles must be re-translated. No "forever" promises.
- **Debuggability.** `golemctl apply --dry-run` shows the translated bundle. What you see is what the agent will reconcile.

---

## 5. Layer 3 — system primitives and state engine

A small set of providers, one engine. This is the heart of the project and where the correctness commitments live.

### Primitives (wire format)

- **`File`** — path, content (base64), mode, owner, group, marker (`Owned` / `Append` / `RegionMarker`).
- **`AptPackage`** — name (and optionally version pin). Idempotent install/remove via `apt-get`.
- **`SystemdUnit`** — name, enable, active, scope (system/user). Idempotent enable/disable + start/stop.
- **`Quadlet`** — name + body of a podman `.container` file dropped in `/etc/containers/systemd/`. The agent writes the body, runs `systemctl daemon-reload`, and the matching `SystemdUnit` claim takes care of enable/active.
- **`NftFragment`** — name + table + raw nft body, dropped into `/etc/nftables.d/<name>.nft` and applied atomically with `nft -f`. Used to materialize `allow_from` firewall rules.
- **`DnsRecord`** — zone, name, kind (A/AAAA/CNAME/TXT/MX), value, ttl, optional priority. Owned by the host carrying the DNS role; the agent calls the configured DNS provider's API. Capture-once-at-first-touch records the prior record (or "did not exist") so removal restores the zone honestly.

Plus the meta:

- **`Handler`** — `source` (a claim id) → `targets` (unit names to restart/reload). Ansible-style debouncing with content_hash gating: a handler fires only if the source's content actually changed this tick.

### State engine: the seven commitments, intact

1. **Level-triggered reconciliation.** Every tick the engine reads desired (from the bundle) and observed (from the system), then mutates toward desired. No diff-based "apply once at deploy" — that's how state drifts.
2. **Refcounted ownership with `preexisting`.** Each claim records who owns it (one or more `owner` strings) and whether it pre-existed Golem's first touch. Removing a claim that pre-existed restores prior state; removing one Golem created removes it; removing one with multiple owners just decrements refcount.
3. **Honest unapply.** Because of (2), removing a claim does the right thing whether it was Golem's or borrowed. Lichess can ask Golem to manage `nginx` on a box where `nginx` was hand-installed five years ago, and Golem will not `apt-get remove nginx` when the claim is unapplied — it will hand it back, untouched.
4. **Journal-before-mutate.** *(See §6 for the corrected provider trait that makes this honest.)* The engine writes its intent and captured prior state to SQLite (WAL, fsync) **before** any OS mutation. A crash mid-mutation always leaves a journal entry that tells the next tick what to do.
5. **Idempotent providers.** `apply` on an already-converged claim is a no-op. `observe` is read-only. `check` returns convergence status without mutation.
6. **Crash-only design.** No graceful shutdown path that's distinct from crash. Kill -9 the agent, restart, converge. The journal is the only state that matters across restarts; in-memory state is rebuilt from disk every tick.
7. **Signed bundles, monotonic version.** Every bundle is ed25519-signed by an operator key trusted by the node. Nodes refuse bundles whose `version` is `<=` the last applied. (See §7 for the TOCTOU fix on the HTTP receive path.)

### Reconcile loop sketch

```
every 30s:
  desired = parse(bundle)                     // layer-3 claims

  // Phase 1: capture-once, BEFORE any mutation.
  for claim in topological_order(desired):
      if not journal.has_capture(claim.id):
          capture = provider.capture(claim.spec)?    // read-only OS snapshot
          journal.put_capture(claim.id, capture)     // FSYNC

  // Phase 2: mutate.
  for claim in topological_order(desired):
      observed = provider.observe()                  // read-only
      if provider.matches(claim.spec, observed): continue
      capture = journal.get_capture(claim.id)
      journal.put_intent(claim.id, Apply, applied=false)   // FSYNC
      provider.mutate(claim.spec, capture)                 // OS mutation
      journal.put_intent(claim.id, Apply, applied=true)    // FSYNC

  // Phase 3: orphan sweep.
  for orphan in observed_owned_by_us - desired:
      capture = journal.get_capture(orphan.id)
      journal.put_intent(orphan.id, Unapply, applied=false) // FSYNC
      provider.unmutate(orphan.last_spec, capture)
      journal.put_intent(orphan.id, Unapply, applied=true)  // FSYNC
      journal.delete_capture(orphan.id)               // re-add gets fresh capture

  fire_handlers(changed_set)
```

See §6 for the crash-invariant analysis of every step.

---

## 6. Correcting journal-before-mutate (the central bug)

The current scaffold's reconciler journals *before* calling `provider.apply`, but each provider mutates its own `state.preexisting` and `state.backup` in memory and only persists *after* the OS mutation completes. A crash between the OS mutation and the post-apply put leaves the journal claiming Golem owns a thing it actually inherited — a future unapply would `apt-get remove` a package the operator installed by hand. This is exactly the failure mode commitment 3 (honest unapply) is meant to prevent.

**Fix: split the Provider trait, capture-once-at-first-touch, capture-then-mutate phased per tick, with a size cap.**

```rust
/// Captured prior state of a single resource. Persisted forever once written;
/// never recomputed. Must fit in MAX_CAPTURE_BYTES.
pub struct Capture {
    /// Did this resource exist before Golem ever touched this claim?
    pub preexisting: bool,
    /// Provider-specific blob. For File: prior content (if ≤ cap), prior mode,
    /// owner, group, hash. For AptPackage: install/hold/version. For SystemdUnit:
    /// active, enabled.
    pub data: Vec<u8>,
}

pub const MAX_CAPTURE_BYTES: usize = 1 << 20; // 1 MiB

trait Provider {
    /// Read-only. Captures everything the engine needs to honor unapply later.
    /// Returns Err(CaptureTooLarge) if the prior state exceeds MAX_CAPTURE_BYTES;
    /// the engine surfaces this as a refused claim, not a silent OOM.
    fn capture(&self, spec: &Spec) -> Result<Capture>;

    /// Read-only. Does the system already match the spec?
    fn matches(&self, spec: &Spec, observed: &Observed) -> bool;

    /// Mutating. Drives system toward spec, given the durable capture as a hint
    /// (e.g., prior content_hash for change-detection, prior_active for restart
    /// suppression). May internally re-observe the OS to converge after a crash.
    /// MUST NOT write to engine state — the engine handles journal writes.
    fn mutate(&self, spec: &Spec, capture: &Capture) -> Result<()>;

    /// Mutating. Reverses mutate, using the captured prior state.
    fn unmutate(&self, spec: &Spec, capture: &Capture) -> Result<()>;

    /// Read-only.
    fn observe(&self) -> Result<Observed>;
}
```

### Capture is durable and one-shot per claim

This is the key correctness property the reviewer caught. Capture for a given `ClaimId` runs **exactly once**, on first touch, and is journaled. After that, the captured `preexisting` and prior state are read from the journal forever — they are never re-derived from observation.

Why: if claim A (`AptPackage caddy`) mutates, the apt postinst writes `/etc/caddy/Caddyfile`. If claim B (`File /etc/caddy/Caddyfile`) then runs `capture` for the first time, it would observe the apt-installed default and record `preexisting=true` — falsely. Honest unapply would then "restore" the apt default instead of removing the file. By making capture one-shot at the bundle's first observation of the claim — and persisting it — this race is closed.

Concretely:

1. The first time a claim's id appears in any bundle the agent applies, `capture` runs *before any mutation in this tick*.
2. The journal entry from that capture is the source of truth forever. Subsequent ticks read it; they never call `capture` again for that id.
3. A claim removed and later re-added gets a *new* capture (different lifecycle).

### Reconcile loop, phased

```rust
// Phase 1: capture all unfamiliar claims, BEFORE any mutation.
//           This snapshots the world as it was before this tick's writes.
for claim in topo_order(&desired) {
    if journal.has_capture(&claim.id) { continue; }
    let capture = provider.capture(&claim.spec)?;          // read-only
    journal.put_capture(&claim.id, &capture)?;             // FSYNC
}

// Phase 2: mutate. Capture for each claim is durable.
for claim in topo_order(&desired) {
    let observed = provider.observe()?;
    if provider.matches(&claim.spec, &observed) { continue; }

    let capture = journal.get_capture(&claim.id)?;
    journal.put_intent(&claim.id, Intent::Apply, applied=false)?;  // FSYNC
    provider.mutate(&claim.spec, &capture)?;                       // OS mutation
    journal.put_intent(&claim.id, Intent::Apply, applied=true)?;   // FSYNC
}

// Phase 3: orphan sweep + handlers (unchanged).
```

### Crash invariants

- **Crash during phase 1 capture (`capture` itself dies):** no journal entry written for that claim. Next tick re-runs phase 1; capture is still honest because no mutation occurred.
- **Crash between `put_capture` and `put_intent(applied=false)`:** capture is durable; next tick sees a captured-but-unintented claim and proceeds to phase 2.
- **Crash during `mutate`:** intent says `applied=false`; next tick re-runs `mutate(spec, capture)`. Mutate re-observes the OS (allowed) and converges. No double-apply, no flap, because providers consult the OS before acting.
- **Crash between `mutate` returning and `put_intent(applied=true)`:** indistinguishable from "mutate didn't fully succeed." Next tick re-runs `mutate`, which is idempotent because providers re-observe.

### Capture size cap

`MAX_CAPTURE_BYTES = 1 MiB` for File providers. A claim like `files."/var/log/foo"` against a 4GB log returns `Err(CaptureTooLarge)`. The engine refuses the claim, logs it, and surfaces it in `/status`. This is a reasonable foot-shot prevention: backing up 4GB on every fleet node is never what the operator wanted, and silently OOM-ing the agent is worse than refusing the claim.

For AptPackage and SystemdUnit, capture is a few hundred bytes max — no cap needed.

### Schema impact

`ClaimState.backup` becomes serializable into `Capture.data`. `preexisting` becomes `Capture.preexisting`. The journal grows a `capture` table keyed by `ClaimId` separate from the intent log.

---

## 7. Wire format and signing

### Canonical JSON

The current scaffold relies on `serde_json::Value` having BTreeMap-sorted keys (which it does, *unless* the `preserve_order` feature is enabled in any dependency). This is a footgun that depends on negative-space behavior of a Cargo feature. Fix: centralize canonicalization in `golem-types::canonical_json` with a property test that asserts byte-equal output across permuted input keys, and explicitly deny `preserve_order` via `[features]` in the workspace Cargo.toml.

### Bundle envelope

```json
{
  "layer3_version": 1,
  "version": 17,
  "node": "app-01",
  "claims": [...],
  "handlers": [...],
  "signature": "ed25519:...",
  "signed_by": "fingerprint-of-operator-pubkey"
}
```

- `layer3_version` is the wire-format generation. Bumps when the engine's claim/journal schema changes shape (e.g., adding per-claim timeouts, capture-size limits, or new provider variants). Agents accept the current version and the previous one; older bundles must be re-translated by a newer `golemctl`. Mismatch is surfaced in `/status` so the operator sees the staleness explicitly rather than getting a silent reject.
- `version` is per-node, monotonically increasing, set by `golemctl`. The agent rejects `version <= last_applied_version` for the node.

### TOCTOU on receive

Current `http.rs` reads `prev_version` then writes the new version with no lock. Two operators racing can land older bundles. Fix: serialize bundle ingest behind a mutex (single-writer for the agent's bundle store), and check `version > prev_version` *inside* the locked section.

### Operator key trust

`/etc/golem/trusted-keys` is a directory of ed25519 public keys. Adding a key to the directory is the trust grant. Removing one is the revocation. No CA, no rotation protocol — small fleets don't need one and adding it now is premature.

---

## 8. Bug list mapped to fixes

(From REVIEW.md, restated as work items.)

| Bug | Fix |
|---|---|
| Journal-before-mutate misimplemented | Provider trait split, capture-once-at-first-touch, `&Capture` into `mutate` (§6) |
| Capture can OOM agent on large prior state | `MAX_CAPTURE_BYTES = 1 MiB` cap, refuse oversize claims explicitly (§6) |
| Apt postinst races with later File capture | Phase-1 capture for *all* claims before *any* phase-2 mutation (§6) |
| `expand_quadlets` drops user-supplied `claim.after` | Merge user `after` with translator-derived edges |
| `http.rs` TOCTOU between version-read and version-write | Mutex around bundle ingest |
| Canonical JSON depends on `preserve_order` being off | Centralize in `golem-types`, property-test, deny feature in workspace |
| `apt-get update` silently failing produces stale install attempts | Treat `apt-get update` non-zero as a hard error; surface in `/status` |
| `systemctl show` parsing is line-based and fragile | Use `systemctl show --property=…` with explicit property list and `=` split, not free-form parsing |
| Daemon-reload heuristic too eager | Only daemon-reload when a `.service`, `.container`, `.volume`, or `.network` file under a systemd-watched path actually changed this tick |
| Orphan sweep removes by `observed - desired` without checking refcount | Sweep must respect refcount: only remove when *all* owners are gone |
| Wire format claimed "stable forever" | `layer3_version` field, two-version compatibility window (§7) |
| Smoke test's kill-9 step was manual | `GOLEM_CRASH_AFTER` env var injection points; smoke test loops over each (§9 M1) |

---

## 9. Milestones, reshaped

### M1 — layer-3 engine, end-to-end, hand-written bundles

Goal: prove the state engine. No Nickel. Hand-write a bundle of File + AptPackage + SystemdUnit claims; install caddy + a Caddyfile + caddy.service; remove it cleanly; verify scripted-crash mid-tick recovery at every injection point.

Acceptance:
- `smoke-test/run.sh` passes the install + remove path.
- `GOLEM_CRASH_AFTER=capture|intent_applied_false|mutate|intent_applied_true` each crash-and-restart leaves the journal honest and the next tick converges.
- Unapplying a claim that pre-existed leaves the system bit-identical to before Golem's first touch (verified by sha256 of relevant files and `dpkg-query`/`systemctl is-active` snapshots).

This is the milestone where the §6 Provider trait fix lands. **No managed control plane in scope here.**

### M2 — layer-1 + layer-2 (the input language)

Goal: tenants write Containers / Ingress / Volumes; translator emits M1 bundles. Tier-1 tenant primitives only.

Acceptance:
- 8-line Nickel deploys a Caddy + app + Postgres stack to a fresh box.
- `golemctl apply --dry-run` prints the translated layer-3 bundle.
- Adding a new container, re-applying, observes only the new claims as changed.
- Tenant Nickel module that tries to use an operator-tier primitive (`files`, `packages`, `units`) fails at `nickel export` with a contract violation.

### M3 — secrets, backups, observability (was M4)

Goal: secrets management (operator-side encryption, agent-side mode-0600 file delivery), volume backup hooks, structured `/status` JSON the operator can scrape with anything.

### M4 — the lichess use case at scale (was M5)

Goal: 30 nodes, 100 services, *operator-coordinated* service migration between boxes. **There is no in-design barrier primitive for "node A must stop X before node B starts X."** Migration is operator-scripted: apply v=N+1 to remove X from A, wait for `/status` on A to confirm removal, then apply v=N+2 to add X to B. This makes the lichess workflow a two-bundle ceremony, not a single-bundle atomic move.

Acceptance:
- Two-bundle migration completes with zero double-running window measured from `/status` polling at 1s granularity.
- Documented runbook: `golemctl migrate <claim> --from <node> --to <node>` is the wrapper script.
- Honest open work: a future `migrate` primitive (M5+) with a real cross-node barrier. Not in M4.

### M5 — managed control plane (deferred from prior plan)

**Deferred and de-scoped.** The original plan made M3 a hosted service that runs `golemctl apply` and forwards signed bundles. This was cut after the §10 budget review: a hosted service with paying tenants (strabs) cannot be maintained inside the 5-hr/month bound, even for a thin signing-and-forwarding service.

If demand justifies it later, the design is preserved as a future milestone:
- Operator's private key stays on their laptop; control plane only sees signed output.
- Hosted plane gets `golemctl push` POSTs; relays to nodes that lack public IPs.
- Pricing: flat $/node/month if it's ever offered.

Until then: operators self-host the control plane (their own laptop, or a small VPS they run themselves). Strabs migration covers infra cost without a hosted offering.

---

## 10. Sustainability

This is a solo project. The success criteria are honest:

- Strabs migration covers infra cost. Confirmed by Lakin. Self-host only — no hosted control plane required to hit this bar.
- OSS adoption brings issue reports and occasional patches. Not a contributor flood; not nothing.
- Maintenance budget target: 8–12 hours/month average post-M2, subject to honest review after each milestone.

The original "5 hours/month" target was challenged in adversarial review and didn't survive. Realistic floor for a Rust agent shelling to `apt-get`/`systemctl`/podman, plus a Nickel translator, plus an axum HTTP server, plus ed25519/SQLite is roughly 8 hrs/month with no users and no bug reports. Add maintenance for one paying customer, multiply. The 8–12 hr budget assumes:

- **No hosted control plane** (M3-was deferred to M5, see §9). This is the single biggest budget cut from the original plan.
- Three providers in layer 3, capped. New layer-1 primitives are added by extending the translator, not the agent. The agent only changes when the engine's correctness contract changes (which means a `layer3_version` bump, see §7).
- Debian trixie only. No matrix testing.
- No SaaS dashboard. `/status` JSON + `curl` is the operator UX.
- No telemetry, no analytics, no auth servers, no SSO. Trusted-keys directory is the entire trust model.

Things that will burn the budget if they happen and we're not ready:

- A `nickel` major version with breaking stdlib changes. Mitigation: pin nickel exactly, upgrade explicitly.
- An `ed25519-dalek` or `axum` major-version churn. Mitigation: pin versions, schedule annual upgrade work.
- podman/Quadlet generator format change. Mitigation: layer-2 owns this; agent doesn't see Quadlet.
- A debian-stable rollover (trixie → forky). Mitigation: explicit per-release cycle, not "we support whatever apt has."

If actual maintenance creeps past 12 hrs/month for two consecutive months, the design has a problem and either scope cuts or contributor recruitment is on the table. Not "push through it."

---

## 11. What this design rejects

- **YAML.** Not even for "simple" things. The cost of YAML's footguns compounds; the cost of Nickel's learning curve does not.
- **Container-only.** Lichess needs `apt install` and unit management alongside containers. Layer 1 keeps escape hatches; the engine treats them as first-class.
- **Diff-based apply.** State drift is real. Level-triggered reconciliation is the only honest answer.
- **A general-purpose CRD model.** Three layer-3 primitives is the design. Adding a fourth requires extending the engine, not just the translator, and that bar is intentionally high.
- **Multi-cluster federation, RBAC, audit logging, policy engines.** Out of scope. If you need these you have outgrown Golem and should run k3s.

---

## 12. Open questions

These need an answer before the milestone they block, not before M1:

1. **Secret-store source of truth.** Local file? `pass`? `age`-encrypted in the repo? Lakin-as-operator will drive this; tenants don't see it. Blocks M3.
2. **Volume sizing enforcement.** `volumes.x.size = "20G"` — advisory or enforced (LVM, quota)? Likely advisory in M2, optional enforcement in M3.
3. **Multi-node ingress.** When two nodes both serve `example.com`, who decides? No managed plane to coordinate; needs an explicit "primary node" or DNS-level decision. Blocks M4.
4. **Backup target.** `volumes.x.backup = "daily"` — to where? Probably a tier-2 `backup_targets.<name>` primitive in M3.
5. **Static IP / DNS.** Out of scope. Operators bring their own DNS.
6. **`migrate` primitive (post-M4).** A real cross-node barrier for atomic service migration. Open: do we add a coordination service (small Raft cluster) or push it to operator scripting forever? Lichess-shaped fleets push for the former; everyone else is fine with the latter.
7. **Layer-3 schema upgrade ceremony.** When `layer3_version` bumps, what's the operator's flow? Re-run `golemctl apply` — does the translator know the agent's current version? Probably the agent advertises it on `/status` and `golemctl` reads it before signing. Blocks the first layer-3 bump (probably M3).
8. **Capture cap policy beyond File.** 1 MiB is right for File. AptPackage and SystemdUnit are tiny. If/when a new provider is added (e.g., NftFragment, CaddySite), we need to revisit per-provider caps. Not blocking M1 because no new providers are in M1–M4.

---

## 13. Status (as of 2026-05-03)

- Original scaffold reviewed in REVIEW.md. Two showstoppers identified, both addressed in this design (§6 journal-before-mutate, §7 wire-format/canonicalization).
- DESIGN.md was then adversarially reviewed. Eight findings absorbed:
  - **Sev 1:** §6 trait now passes `&Capture` to `mutate`, capture is one-shot at first touch and durable, `MAX_CAPTURE_BYTES` cap on File capture, phase-1-then-phase-2 reconcile loop.
  - **Sev 2:** `layer3_version` added to bundle envelope (§7). M5 (now M4) explicit that there is no in-design migration barrier — it's an operator-scripted two-bundle ceremony, with a `migrate` primitive flagged as future open work.
  - **Sev 3:** Hosted control plane cut from the active milestone ladder (was M3, now deferred to M5). Layer-1 split into two trust tiers (§3): tenant-tier for containers/ingress/volumes/secrets, operator-tier for files/packages/units. Strabs tenants can't write `/etc/sudoers`.
  - **Sev 4:** M1 acceptance now requires scripted `GOLEM_CRASH_AFTER` injection at every crash window, not a manual step.
- M1 smoke test (drafted in `smoke-test/run.sh`) is ready in skeleton; needs the `GOLEM_CRASH_AFTER` injection loop added once the §6 fix lands in code.
- The 7 commitments in §5 survive intact. The three-layer model in §2 is the canonical mental model going forward. The README's M1–M5 ladder is replaced by §9.
