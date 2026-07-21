# 0023-container-service-abstraction

## Status

Proposed 2026-07-21; for review, do NOT implement. An Emet **library**
abstraction only — it adds no golemd resource kind, and reuses the four glyphs
of ADR 0002 plus the directory arm of ADR 0019. Recommends **superseding** the
lichess `Workload`/`Service`/`Ingress` shapes (`examples/lichess/Lichess.emet`).

## Context

golem now dogfoods real containers: a container **registry**
(`examples/registry/registry.emet`) and a website Caddy container. Both were
hand-rolled — a `String.join`'d quadlet `.container` file, a `podman`
`aptPackage`, and a `systemdService`, with the registry adding a `PublishPort`
and a `Volume=…:Z` line and its clients an insecure-registry drop-in `file`
(`examples/registry/clients.emet`). The one existing library abstraction that
was supposed to cover this, the lichess `Workload`, emits a quadlet that is
**networkless, storageless, and env-less** (`quadletContents` hard-codes only
`Image`, `ContainerName`, and `Restart=always`). Its `Service` adds a firewall
`file` but still no ports/volumes/env on the container; its `Ingress` is an
nginx front door unrelated to the container's own runtime shape.

The dogfoods proved the gap concretely. A real service needs, at minimum:

- **Published ports** — the registry publishes `5000:5000`; a web service
  publishes `443` / `80`. Today only a bespoke `PublishPort=` line, and the
  firewall opening is a *separate* `Service`/`Ingress` concern that can drift
  out of sync with what the container actually publishes.
- **Volumes** — the registry needs `golem-registry-data:/var/lib/registry:Z`
  (a **named** volume). Other services need a **host-path** bind mount whose
  source directory golem must create first — now possible because the
  filesystem glyph gained a `directory` arm (ADR 0019). Podman `statfs`-refuses
  a bind mount whose source is absent, so the source `directory` glyph and the
  `Volume=` line must be emitted together, from one place.
- **Environment variables** — a web service is configured by `Environment=`
  lines; the current `Workload` has nowhere to put them.
- **A restart policy** — hard-coded to `always`; `on-failure` and `no` are
  legitimate and are a closed set, not free text.

The tension to resolve, with Elm-shaped rigor (**make illegal states
unrepresentable**, Dr. Dub's standing review ask — the same discipline ADR 0019
applied to the filesystem `Entry` sum):

- **What is the type of a service?** One record carrying all four concerns, or
  a scatter of per-concern helpers the author must remember to wire together
  (as today, where the firewall opening is manually kept in step with the
  published port)?
- **Volumes are genuinely two different things** — a named podman volume (no
  host path) and a host-path bind mount (needs a source `directory` glyph, and
  may be read-only or `:Z`-relabeled). A single record with an optional
  `source` field makes the named-with-a-source-path and host-path-without-a-
  source combinations representable and meaningless. That is exactly a sum.
- **Restart is a closed enum**, not a `String` — `Restart=alwyas` should not be
  expressible.
- **Does a service carry its own exposure**, folding in the old
  `Service`/`Ingress` firewall openings, or is ingress a separate abstraction?

Constraints that bound any answer:

- **Nothing new in golemd** (root `CLAUDE.md`, ADR 0002/0019): this is Emet
  values evaluating to `List Glyph`. The container is a quadlet `.container`
  **`file`**, the runtime is a `podman` **`aptPackage`**, the unit is a
  **`systemdService`**, a host-path volume source is a **`directory`**, and a
  firewall opening is an nftables **`file`** (and, for a shared chain, a
  **`lineInFile`**) — the same five glyph spellings the dogfoods already use by
  hand.
- **Emet has ADTs and records** (parameterized `type`, ADR 0016) and pattern
  matching with exhaustiveness (ADR 0005), `Maybe` (prelude), and `List`. It
  has **no tuple type** — so the task's sketched `env : List (String, String)`
  is not expressible; env pairs must be a named record (a small but real
  modeling decision, see Decision §1).
- **The wire format is untouched.** No glyph field or variant changes; this ADR
  is entirely above the glyph layer, so there is **no `format_version` bump**
  (contrast ADR 0019, which changed the wire).

## Decision

Introduce a **`Container` service abstraction** in an Emet library module
(`Container.emet`), shaped as an Elm record of a small closed set of concerns,
each concern its own correct-by-construction ADT, lowering to the four glyphs.
**Replace** the lichess `Workload`/`Service`/`Ingress` with it (see §5).

### 1. The `Container` record and its concern types

```elm
type Container = Container
  { name    : String
  , image   : String
  , ports   : List Port
  , volumes : List Volume
  , env     : List EnvVar
  , restart : Restart
  , expose  : Expose
  }
```

`Container` is one nominal record (an ADT with a single constructor, the ADR
0019 §2 pattern for "a value the library owns, not an open `{…}`"). Every
concern is a list or a closed enum — never a `Maybe`-riddled flat record — so
an empty service is `ports = []`, `volumes = []`, `env = []`, not a pile of
`Nothing`s. The four runtime concerns plus **exposure** (§3) sit together, so
the firewall opening is *derived from the published ports*, not hand-kept in
step with them as today.

**Ports** — published, container, protocol; the protocol is a closed enum, not
`"tcp"`/`"udp"` strings:

```elm
type Proto = TCP | UDP
type Port  = Port { published : Int, container : Int, proto : Proto }
```

A convenience `tcp : Int -> Int -> Port` (and `udp`) keeps the common case
terse; `Port { published = 5000, container = 5000, proto = TCP }` lowers to
`PublishPort=5000:5000/tcp`.

**Volumes** — the honest sum. A named volume and a host-path bind mount are two
different things; each arm carries only its own fields:

```elm
type Mount  = ReadWrite | ReadOnly     -- :ro
type Relabel = Shared | Private | NoRelabel  -- :z / :Z / (none)

type Volume
  = Named    { name : String, at : String, mount : Mount, relabel : Relabel }
  | HostPath { source : String, at : String, mount : Mount, relabel : Relabel }
```

- `Named { name, at }` lowers to `Volume=<name>:<at>[:opts]` and **emits no
  directory glyph** — podman creates the named volume.
- `HostPath { source, at }` lowers to `Volume=<source>:<at>[:opts]` **and emits
  a `directory { path = source, mode = "0755" }` glyph** (ADR 0019) so the bind
  mount source exists before the unit starts. This is the whole reason ADR 0019
  landed; this abstraction is its first consumer.
- **`:Z`/`:z` and `:ro` are modeled, minimally-correctly, as the two small
  enums `Relabel` and `Mount`**, not as free-text option strings. The option
  suffix is *computed* from them (`ReadOnly` → `ro`, `Shared` → `z`, `Private`
  → `Z`), so `Volume=…:Z:rw:garbage` cannot be written. `at` (the in-container
  mount point) is required on both arms; a volume with no mount point is
  meaningless and unrepresentable. The `source`/`name` distinction — the field
  that decides "is there a host directory to create?" — is carried by *which
  arm*, so the "named volume with a stray host path" and "host path with no
  source" states cannot exist.

**Env** — a named record, because Emet has no tuples:

```elm
type EnvVar = EnvVar { name : String, value : String }
```

lowering to `Environment=<name>=<value>`. (Rejected the task's
`List (String, String)`: no tuple type exists in Emet — Context — and a named
record reads better at the call site anyway: `EnvVar { name = "PORT", value =
"443" }`.)

**Restart** — a closed enum, ADR 0019's "not a string" discipline applied to
the systemd restart policy:

```elm
type Restart = Always | OnFailure | No
```

lowering to `Restart=always` / `on-failure` / `no`. `Restart=alwyas` is now a
type error, not a silently-broken unit.

### 2. Exposure — how the old `Service`/`Ingress` firewall folds in

`Service` (internal firewall opening) and `Ingress` (public front door)
conflated *two* questions that a service should answer as *one*: **who may
reach the ports this container publishes?** Model that as the service's own
field, a closed enum:

```elm
type Expose
  = Unexposed              -- published only on the host loopback / no firewall glyph
  | Internal               -- opened to the fleet CIDR (Fleet.internalNetwork)
  | Public                 -- opened to the world
```

- `expose = Unexposed` emits **no** firewall glyph.
- `expose = Internal` emits an nftables `file` opening **each published port**
  to `Fleet.internalNetwork` (the old `Service` behaviour — but derived from
  `ports`, so it cannot fall out of step with them).
- `expose = Public` emits an nftables `file` opening each published port to the
  world, plus the shared-chain `lineInFile` (the old `Ingress` firewall
  behaviour), for every published port.

Crucially, **exposure is derived from `ports`**: you cannot open a port the
container does not publish, and you cannot publish a port and forget to open
it. That is the illegal state the current split (a `Service.port : Int` beside
a quadlet with no port at all) actively invites.

**The nginx/Caddy reverse-proxy front door is deliberately NOT folded in.** A
TLS terminator is *itself a `Container`* (Caddy is the website's container) with
`expose = Public` and `ports = [ tcp 443 443, tcp 80 80 ]`. The old `Ingress`'s
nginx-site `file` is not a property of the upstream service — it is the config
of a *different* container. Reverse proxy configuration is left to a thin
follow-on (a `caddyfile`/`site` helper that emits a `file` for the proxy
container), out of scope here. This ADR covers the container runtime shape and
its firewall exposure; it does not re-encode nginx-specific site templating.

### 3. Glyph lowering

`containerGlyphs : Container -> List Glyph` produces, in order:

1. `aptPackage { name = "podman" }` — the runtime.
2. one `directory { path = source }` glyph **per `HostPath` volume** (ADR 0019),
   so bind-mount sources exist first. Named volumes contribute none.
3. `file { path = "/etc/containers/systemd/<name>.container", contents = <quadlet>,
   mode = "0644" }` — the quadlet, whose `[Container]` section is assembled from
   the record: one `Image=`, one `PublishPort=` per `Port`, one `Volume=` per
   `Volume`, one `Environment=` per `EnvVar`, and `Restart=` from `restart` in
   `[Service]`.
4. `systemdService { unit = "<name>.service" }` — the generated unit.
5. zero or more firewall glyphs from `expose` (§2): an nftables `file` when
   `Internal`/`Public`, plus a shared-chain `lineInFile` when `Public`.

Every line of the quadlet is *computed from typed data*, so the "PublishPort but
no firewall" and "Volume with no source directory" drift the dogfoods hit are
structurally impossible. The lowering is ordinary `List.map`/`List.concat` over
the concern lists — no templating, concrete strings only (ADR 0004).

### 4. What stays golem-side vs Emet-side

- **golem-side: nothing new.** No fifth glyph, no reconciler, no wire change.
  The five glyph spellings used (`aptPackage`, `directory`, `file`,
  `systemdService`, `lineInFile`) all already exist and already reconcile. This
  is the ADR 0002/0019 invariant working as intended: a new *capability*
  (a full container service) is a new *library value*, not a new golemd kind.
- **Emet-side: one library module** (`Container.emet`) exposing the `Container`
  constructor, the concern types (`Port`/`Volume`/`EnvVar`/`Restart`/`Expose`
  and their arms, open-exposed with `(..)` so callers can build them, ADR 0016),
  the `tcp`/`udp`/named-volume/host-path smart constructors, and
  `containerGlyphs : Container -> List Glyph`.

### 5. Re-expressing the dogfoods; replacing the lichess shapes

**Recommend replacing** `Workload`/`Service`/`Ingress`. `Workload` is exactly
`Container` with empty lists and `Unexposed`; `Service` is `Container` with
`expose = Internal`; `Ingress`'s *firewall* half is `expose = Public` and its
*nginx* half is a separate proxy-container concern (§2). One type subsumes all
three, with the ports/volumes/env the old three lacked. The lichess module
becomes a thin re-export (or is deleted) once its `fleet.emet` is ported.

The **registry** re-expressed:

```elm
registry : Container
registry = Container
  { name = "registry"
  , image = "docker.io/library/registry:2"
  , ports = [ Port { published = 5000, container = 5000, proto = TCP } ]
  , volumes =
      [ Named { name = "golem-registry-data", at = "/var/lib/registry"
              , mount = ReadWrite, relabel = Private } ]  -- :Z
  , env = []
  , restart = Always
  , expose = Internal
  }
```

`containerGlyphs registry` yields exactly the podman apt + quadlet
(`PublishPort=5000:5000/tcp`, `Volume=golem-registry-data:/var/lib/registry:Z`,
`Restart=always`) + service the hand-rolled `registry.emet` built, plus the
internal firewall opening it lacked — no `String.join` in the example. The
**website Caddy** container is the same shape with `expose = Public`, TLS ports,
and a `HostPath` volume for its config/data (whose `directory` source glyph is
now emitted for free). The insecure-registry drop-in on *clients*
(`clients.emet`) is unrelated to running a container and stays a plain `file`
(or a tiny separate `registryClient` helper) — this ADR does not absorb it.

## Alternatives considered

1. **A thin flat record with optional fields**
   (`{ image, port : Maybe Int, volume : Maybe String, source : Maybe String,
   env : List …, readOnly : Bool, relabel : Maybe String }`). Rejected — it is
   the illegal-states trap ADR 0019 already rejected for the filesystem entry.
   `port : Maybe Int` allows only one port (services publish several);
   `volume : Maybe String` + `source : Maybe String` makes "named volume with a
   host source" and "host path with no source" representable and meaningless;
   `relabel : Maybe String` re-admits `":garbage"`. The sum-of-concerns model
   pushes each field onto the one arm that gives it meaning, exactly as
   `Entry`'s arms carry only their valid fields.

2. **Per-concern glyph helpers** (`portGlyph`, `volumeGlyph`, `envLine`, … that
   the author wires together by hand). Rejected — this is essentially *today*,
   and it is what let the registry's `PublishPort` and its firewall opening live
   in two unrelated places and drift. The value of the abstraction is precisely
   that exposure is *derived from* ports and a host-path volume's `directory` is
   *derived from* the volume, in one place, by construction.

3. **A golemd-side `container` glyph kind** (a fifth reconciler that takes
   `image`/`ports`/`volumes`/… and runs podman itself). Rejected outright — it
   violates the four-glyph model (root `CLAUDE.md`, ADR 0002): golemd would
   grow podman/quadlet knowledge, a new `key()` namespace, a new `Inverse`, and
   a `format_version` bump, to express what composes cleanly from the four
   glyphs it already reverses. Containers are a *library* abstraction that
   compiles down, never a new kind — the same line ADR 0019 held for
   directories.

4. **`env : List (String, String)`** (the task's sketch). Rejected mechanically:
   Emet has no tuple type (Context). `EnvVar { name, value }` is the
   representable and more readable form.

5. **Keep `Ingress` as the nginx front door and only add ports/volumes/env to
   `Workload`.** Rejected as half the fix: it leaves the port/firewall drift in
   place (a `Service.port` beside a quadlet with no port) and keeps two shapes
   where one `Container` with an `Expose` field is clearer. The reverse-proxy
   *config* genuinely is a separate follow-on, but the *firewall exposure*
   belongs on the service that owns the ports.

## Consequences

- **A container service is one typed value** carrying ports, volumes, env,
  restart, and exposure, lowering to the four glyphs. The registry and website
  dogfoods re-express without hand-written `String.join` quadlets, and gain the
  firewall opening (registry) and host-path source directory (website) they
  previously lacked or hand-rolled.
- **Illegal service states are unrepresentable** (Dr. Dub's review ask): the
  named-vs-host-path volume distinction is an ADT arm (no stray `source`), the
  `:Z`/`:ro` options are closed enums (no free-text suffix), the restart policy
  is an enum (no `alwyas`), and firewall exposure is *derived from* the
  published ports (no open-a-port-you-don't-publish, no publish-and-forget).
- **`directory` (ADR 0019) gets its first library consumer**: a `HostPath`
  volume emits its bind-mount source directory automatically, closing the podman
  `statfs`-refusal gap without the author remembering to add a `directory`
  glyph.
- **No golemd change and no `format_version` bump.** This is entirely an Emet
  library above the glyph layer; the wire contract (ADR 0012/0013) is untouched.
  golem still owns exactly the four glyphs.
- **Replacing `Workload`/`Service`/`Ingress` is a source migration** of
  `examples/lichess/` and the registry/website examples — no compiler work, but
  a breaking change to any fleet importing the old shapes (they are dogfoods,
  so the cost is a re-author, per the ADR 0019 disposable-fleet stance).
- **Forecloses** growing container capability by widening a flat record with
  optional fields; commits the library to the "each concern is its own
  minimal-per-arm ADT, exposure derived from ports" model. Adding a concern
  later (healthcheck, network alias, cpu/memory limits) is a new field of its
  own closed type, matching this ADR's path.
- **Cross-references:** composes the four glyphs of ADR 0002 and the `directory`
  arm of ADR 0019; authored with the parameterized ADTs of ADR 0016 and the
  exhaustive `case` of ADR 0005; supersedes the lichess `Workload`/`Service`/
  `Ingress` of `examples/lichess/Lichess.emet`.

## Open decisions for review

- **Reverse-proxy config placement.** This ADR deliberately leaves the
  nginx/Caddy *site* config out (a proxy is itself a `Container`). Is a
  follow-on `proxy`/`site` helper wanted, or should the abstraction carry an
  optional `proxyFor : Maybe String` and emit a Caddyfile? (Recommendation:
  separate follow-on — keep `Container` about the runtime, not proxy templating.)
- **`Expose` granularity.** Is the three-way `Unexposed | Internal | Public`
  enough, or do we need per-port exposure (some ports internal, some public on
  one container)? (Recommendation: start with whole-container `Expose`; promote
  to `Port`-level only if a dogfood needs it — YAGNI over a `List (Port,
  Expose)` that Emet's lack of tuples would make a record anyway.)
- **Named vs. host-path default relabel.** The registry used `:Z` (`Private`).
  Should `relabel`/`mount` have defaults via smart constructors (`namedVolume
  name at = Named { …, mount = ReadWrite, relabel = Private }`), or always be
  explicit? (Recommendation: smart constructors for the common case, explicit
  record for the rest.)
- **Does `clients.emet`'s insecure-registry drop-in belong in this library**
  (a `registryClient` helper) or stay a bare `file` in the example?
  (Recommendation: out of scope — it configures the podman *client*, not a
  running container.)
</content>
</invoke>
