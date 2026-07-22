# 0023-quadlet-based-workload-abstraction

## Status

Proposed 2026-07-22; **for review, do NOT implement.** An Emet **library**
abstraction only — it adds no golemd resource kind, changes no wire format, and
bumps no `format_version`. It reuses the four glyphs of ADR 0002 plus the
`directory` arm of ADR 0019. It recommends **superseding** the thin lichess
`Workload`/`Service`/`Ingress` shapes (`examples/lichess/Lichess.emet`) with a
strongly-typed model of the thing golem actually runs: **Podman Quadlets**.

This rewrites the first draft, which invented a generic `Container` record.
Dr. Dub rejected that shape: it modeled a made-up abstraction instead of the
real artifact. This version models Quadlets directly — a `.container` unit and
the separate `.volume` unit it references — so the types mirror the files golem
writes, not an invented middle layer.

## Scope — stated up front, on purpose

**This library is Debian + systemd + Podman-Quadlet specific, deliberately.**
It is not generic over container runtimes (no Docker, no containerd, no
Kubernetes), not generic over service managers (systemd only), and not generic
over OSes (Debian `apt` + `/etc/containers/systemd/` layout only). It models the
concrete quadlet key/value surface that Podman's `podman-systemd.unit(5)`
generator consumes, and it lowers through Debian package + systemd unit + nftables
conventions.

The generic layer is elsewhere and stays generic: the **four glyphs** (ADR 0002,
ADR 0019) are the OS-agnostic primitives golemd reconciles. This library sits
*above* them and is intentionally opinionated. A future runtime or OS would be a
*different* library targeting the same four glyphs — a later design if ever
needed, explicitly out of scope here. We are modeling Quadlets well, not
abstracting over "containers in general."

## Context

golem now dogfoods real containers, all hand-rolled as quadlets:

- The **registry** (`examples/registry/registry.emet`) — a
  `String.join`'d `.container` file with `Image=`, `ContainerName=`,
  `PublishPort=5000:5000`, `Volume=golem-registry-data:/var/lib/registry:Z`, plus
  a `podman` `aptPackage` and a `systemdService`.
- The **website** Caddy/web box (`examples/website/website.emet`) — the same
  hand-rolled quadlet shape with `PublishPort=80:80`, plus an insecure-registry
  drop-in `file`.
- Their **clients** (`examples/registry/clients.emet`) — the insecure-registry
  drop-in only, no container.

The one existing library abstraction meant to cover this, the lichess `Workload`,
emits a quadlet that is **networkless, storageless, and env-less**:
`quadletContents` hard-codes only `Image`, `ContainerName`, and `Restart=always`.
Its `Service` bolts on a firewall `file` but adds no ports/volumes/env to the
container; its `Ingress` is an nginx front door unrelated to the container's own
runtime shape. The dogfoods could not use it, so they hand-rolled strings.

Those hand-rolled strings are where the real problems live, and they map exactly
onto quadlet reality:

- **A `.container` unit has an `Image`, an `Exec`, `PublishPort=` lines,
  `Volume=` lines, `Environment=` lines, and a `Restart=`** — every one of them a
  `String.join`'d line today, with no type stopping `Restart=alwyas` or
  `PublishPort=5000:5000:oops`.
- **A volume is not a field of the container — it is its own quadlet.** A named
  podman volume is a separate `.volume` unit (`golem-registry-data.volume`) that
  the `.container` references by a `Volume=` mount line. A host-path bind mount
  has no `.volume` unit at all but needs its **source directory to exist first**
  (podman `statfs`-refuses an absent bind source — the entire reason ADR 0019
  added the `directory` glyph). The current registry pretends the named volume is
  just a string on the container. It is not.
- **The firewall opening is a separate concern that drifts.** The registry
  publishes `5000` but its firewall rule (when it has one) is hand-kept in a
  separate `Service`; the published port and the opened port can silently
  disagree. Firewall exposure should be *derived from the published ports* so the
  two cannot drift — the one genuinely good idea from the first draft, kept here.

The tension to resolve, with Elm-shaped rigor (**make illegal states
unrepresentable**, Dr. Dub's standing review ask, the same discipline ADR 0019
applied to the filesystem `Entry` sum):

- **What is the type of a quadlet?** Not one god-record with optional fields, but
  a faithful model of the two unit kinds — a `.container` and a `.volume` — where
  the `.container` *references* volumes rather than containing them, mirroring the
  files on disk.
- **`image` is not a `String`.** `docker.io/library/registry:2` has a registry,
  a name, and a tag-or-digest; `registry@sha256:…` pins a digest. A `String`
  admits `"registry:2:latest"` and typos in the digest algorithm. An `Image`
  type makes the structure explicit and the illegal spellings unwritable.
- **A mount is a sum, not a record with optional fields.** A mount from a named
  volume (references a `.volume` unit, no host path) and a mount from a host path
  (needs a source `directory` glyph, no `.volume` unit) are two different things;
  a single record with an optional `source` makes "named volume with a stray host
  path" and "host path with no source" representable and meaningless. That is a
  sum.
- **`Restart`, `Proto` are closed enums**, not `String`s.

Constraints that bound any answer:

- **Nothing new in golemd** (root `CLAUDE.md`, ADR 0002/0019): this is Emet
  values evaluating to `List Glyph`. Every quadlet unit is a filesystem `file`,
  the runtime is a `podman` `aptPackage`, the generated unit is a
  `systemdService`, a host-path mount source is a `directory` (ADR 0019), and a
  firewall opening is an nftables `file` (and, for a shared chain, a
  `lineInFile`) — the same glyph spellings the dogfoods already use by hand.
- **Emet has ADTs and records** (parameterized `type`, ADR 0016), exhaustive
  `case` (ADR 0005), `Maybe`, and `List`. It has **no tuple type** — so
  `env : List (String, String)` is not expressible; env pairs are a named
  `EnvVar` record.
- **The wire format is untouched.** No glyph field or variant changes; this ADR
  is entirely above the glyph layer, so there is **no `format_version` bump**
  (contrast ADR 0019, which changed the wire).

## Decision

Ship an Emet library that models **Podman Quadlets as strongly-typed values** —
a `ContainerUnit` (the `.container`) and a `VolumeUnit` (the `.volume`), each
field a real quadlet key — and expose `Workload` as the ergonomic top-level
surface that carries `env` and lowers to a `ContainerUnit` plus its
`VolumeUnit`s, which lower to the four glyphs. `.network`/`.pod` are named as
future unit kinds, not modeled now.

### 0. Three layers — stated explicitly

This ADR is deliberately about the *middle* layer. Name all three so the
boundary is clear:

- **(a) The four glyphs** — `aptPackage`, `systemdService`, `file` (with the
  `directory`/`symlink` arms of ADR 0019), `lineInFile`. Generic, OS-agnostic,
  golemd-owned primitives. This ADR adds nothing here.
- **(b) The strongly-typed quadlet library golem SHIPS** — `Image`, `Port`,
  `Proto`, `EnvVar`, `Restart`, `Mount`, `VolumeUnit`, `ContainerUnit`, and the
  `Workload` surface over them. **This is this ADR's core deliverable.** It is
  Podman/systemd/Debian-specific (see Scope) and lowers entirely to (a).
- **(c) Ergonomic helpers on top** — lichess-style shortcuts for a fleet's common
  patterns (e.g. "a public TLS web service", "an internal HTTP service"). These
  are **ordinary user Emet code built on (b)**, **NOT part of the shipped
  library**. This ADR *frames* them and sketches an example (§6), but does not
  ship them; each fleet writes its own.

The design work below is (b). (a) is reused as-is; (c) is user-land with an
illustrative sketch.

### 1. `Image` — a typed image reference

`image` is never a `String`. An image reference is a registry, a repository
name, and a **tag or digest** (never both, never neither):

```elm
type Ref
  = Tag String         -- :2, :latest
  | Digest String      -- @sha256:…  (the algo:hex, validated by the smart ctor)

type Image = Image
  { registry : String  -- "docker.io"
  , name     : String  -- "library/registry"
  , ref      : Ref
  }
```

- `Ref` is a sum, so an image is tagged **xor** digest-pinned — `registry:2` and
  `registry@sha256:…` are both expressible, `registry:2@sha256:…` and a bare
  reference with neither are not.
- Smart constructors keep the common case terse and parse the string once:
  `image "docker.io/library/registry:2"` and `imageAt "…" "sha256:…"`. The
  registry defaults are the library's, not free text at every call site.
- Lowers to the single `Image=` line: `Tag t` → `<registry>/<name>:<t>`,
  `Digest d` → `<registry>/<name>@<d>`.

(Open question §Open: exactly how much of a reference to parse/validate now vs.
keep as a lightly-typed 3-field record.)

### 2. `Port`, `Proto` — published ports

```elm
type Proto = TCP | UDP
type Port  = Port { host : Int, container : Int, proto : Proto }
```

- `Port { host = 5000, container = 5000, proto = TCP }` lowers to one
  `PublishPort=5000:5000/tcp`.
- Smart constructors `tcp : Int -> Int -> Port` and `udp` for terseness.
- The protocol is a closed enum, so `…/tpc` is unwritable.

### 3. `EnvVar` — environment, as a named record

Emet has no tuples (Context), so an env pair is a named record, which reads
better at the call site anyway:

```elm
type EnvVar = EnvVar { name : String, value : String }
```

lowering to one `Environment=<name>=<value>` line per entry.

### 4. `Restart` — the systemd restart policy, closed

```elm
type Restart = Always | OnFailure | No
```

lowering to `Restart=always` / `on-failure` / `no`. `Restart=alwyas` is a type
error, not a silently-broken unit — ADR 0019's "not a string" discipline applied
to the quadlet key.

### 5. `Mount` and `VolumeUnit` — the volume is its own quadlet

This is the correction at the heart of the rewrite. A `.container` does **not**
"have volumes." It has **mount lines** (`Volume=SOURCE:AT[:opts]`) that reference
either a named `.volume` unit or a host path. The named volume is a *separate
quadlet unit*.

```elm
type Access  = ReadWrite | ReadOnly            -- ""  / :ro
type Relabel = NoRelabel | Shared | Private    -- ""  / :z  / :Z

-- One mount LINE on a .container. A sum, because a named-volume mount and a
-- host-path mount are genuinely different: the first names a .volume unit and
-- emits no directory; the second names a host path and MUST emit its source
-- directory (ADR 0019).
type Mount
  = FromVolume { volume : String, at : String, access : Access, relabel : Relabel }
  | FromHost   { source : String, at : String, access : Access, relabel : Relabel }

-- The .volume quadlet itself. Referenced by a FromVolume mount's `volume` name.
type VolumeUnit = VolumeUnit
  { name    : String        -- <name>.volume; the podman volume is systemd-<name>
  , driver  : String        -- "local"
  }
```

- `FromVolume { volume, at }` lowers to `Volume=<volume>.volume:<at>[:opts]` on
  the container and **references a `VolumeUnit`** — a separate
  `<volume>.volume` `file`. It emits **no** `directory` glyph (podman owns the
  named volume's storage).
- `FromHost { source, at }` lowers to `Volume=<source>:<at>[:opts]` and emits a
  `directory { path = source, mode = "0755" }` glyph (ADR 0019) so the bind-mount
  source exists before the unit starts. **No `.volume` unit** — a host path is not
  a podman volume. This is ADR 0019's first library consumer.
- `at` (the in-container mount point) is required on both arms; a mount with no
  mount point is meaningless and unrepresentable.
- `:ro`/`:z`/`:Z` are **computed** from the `Access`/`Relabel` enums, so
  `Volume=…:Z:rw:garbage` cannot be written. The "named volume with a stray host
  path" and "host path with no source" states cannot exist, because *which arm*
  carries the field that decides "is there a host directory / a `.volume` unit?"

### 6. `ContainerUnit` — the `.container` quadlet, strongly typed

Each field is a real quadlet key from the `[Container]`/`[Service]`/`[Install]`
sections. This is the model of the file on disk:

```elm
type ContainerUnit = ContainerUnit
  { name         : String        -- <name>.container, ContainerName=<name>
  , image        : Image         -- Image=
  , exec         : Maybe String  -- Exec=  (Nothing = image default)
  , publishPorts : List Port     -- PublishPort= (one per Port)
  , mounts       : List Mount    -- Volume=      (one per Mount)
  , volumeUnits  : List VolumeUnit-- the .volume units the mounts reference
  , environment  : List EnvVar    -- Environment=
  , restart      : Restart        -- [Service] Restart=
  , wantedBy     : List String    -- [Install] WantedBy=  (the install target)
  }
```

- `ContainerUnit` is a nominal record (single-constructor ADT, the ADR 0019 §2
  pattern for "a value the library owns"). Empty concerns are `[]`, not a pile of
  `Nothing`s. `exec` is a `Maybe` because "no `Exec=` line, use the image
  default" is a real, distinct state from "run this command."
- `volumeUnits` sits **beside** `mounts` because the `.volume` files are separate
  quadlets that must be emitted whether or not you look only at the container;
  a `FromVolume` mount is a *reference* to one of them by name. (Open question
  §Open: derive `volumeUnits` from the `FromVolume` mounts automatically, vs.
  carry them explicitly so a volume can set a non-default driver.)
- The install target (`WantedBy=multi-user.target default.target`) is data, not a
  hard-coded string buried in a `String.join`.

### 7. `Workload` — the ergonomic top-level (keeps its name, gains `env`)

`Workload` stays the user-facing name (Dr. Dub's ask) and is the ergonomic
surface most fleets author. It is a flatter record that **lowers to a
`ContainerUnit` plus its `VolumeUnit`s**, and it **carries `env`** — the field
the old `Workload` lacked:

```elm
type Workload = Workload
  { name    : String
  , image   : Image
  , env     : List EnvVar
  , ports   : List Port
  , volumes : List Mount     -- FromVolume / FromHost
  , restart : Restart
  , expose  : Expose
  }
```

`workloadUnits : Workload -> ContainerUnit` (plus the referenced `VolumeUnit`s)
maps the flat surface onto the faithful quadlet model:

- `image`/`env`/`ports`/`restart` map straight through to the `ContainerUnit`
  fields (`env` → `environment`, `ports` → `publishPorts`).
- `volumes` become the container's `mounts`; every `FromVolume` mount
  contributes a `VolumeUnit` (default `driver = "local"`) to `volumeUnits`.
- `wantedBy` and `exec` take library defaults (`Exec = Nothing`, the standard
  install target) — a `Workload` is the common case; reach for `ContainerUnit`
  directly for the uncommon one.

**Exposure derived from ports.** `Workload` folds in the old
`Service`/`Ingress` firewall openings as a closed enum answering *one* question —
who may reach the ports this container publishes? — **derived from `ports`**, so
publish and firewall cannot drift:

```elm
type Expose = Unexposed | Internal | Public
```

- `Unexposed` → no firewall glyph.
- `Internal` → an nftables `file` opening **each published port** to
  `Fleet.internalNetwork` (the old `Service` behaviour, now derived from `ports`).
- `Public` → an nftables `file` opening each published port to the world, plus
  the shared-chain `lineInFile` (the old `Ingress` firewall half), per port.

You cannot open a port the container does not publish, and you cannot publish a
port and forget to open it — the illegal state the current split (a
`Service.port : Int` beside a quadlet with no port) invites.

**The nginx/Caddy reverse-proxy front door is deliberately NOT folded in.** A TLS
terminator is *itself a `Workload`* (Caddy is the website's container, with
`expose = Public` and `ports = [ tcp 443 443, tcp 80 80 ]`). The old `Ingress`'s
nginx-site `file` is the config of a *different* container, not a property of the
upstream — so reverse-proxy templating is a separate concern (see §Open), not
part of this library.

### 8. Glyph lowering

`workloadGlyphs : Workload -> List Glyph` (via `workloadUnits` →
`containerGlyphs : ContainerUnit -> List Glyph`) produces, in order:

1. `aptPackage { name = "podman" }` — the runtime.
2. one `directory { path = source, mode = "0755" }` glyph **per `FromHost`
   mount** (ADR 0019), so bind-mount sources exist first. Named volumes
   contribute none.
3. one `file { path = "/etc/containers/systemd/<name>.volume", … }` **per
   `VolumeUnit`** — the `.volume` quadlets the container references.
4. `file { path = "/etc/containers/systemd/<name>.container", contents =
   <rendered from the typed ContainerUnit>, mode = "0644" }` — the `.container`
   quadlet: one `Image=`, one `PublishPort=` per `Port`, one `Volume=` per
   `Mount`, one `Environment=` per `EnvVar`, `Restart=` from `restart`,
   `WantedBy=` from `wantedBy`.
5. `systemdService { unit = "<name>.service" }` — the unit the generator produces
   from the `.container`.
6. zero or more firewall glyphs from `expose` (§7): an nftables `file` when
   `Internal`/`Public`, plus a shared-chain `lineInFile` when `Public`.

Every line of every quadlet is *computed from typed data* — ordinary
`List.map`/`List.concat` over the concern lists, concrete strings only (ADR
0004), no templating engine. The "PublishPort but no firewall" and "Volume with
no source directory" drift the dogfoods hit are structurally impossible.

### 9. Layer (c) — ergonomic helpers as user code (example, not shipped)

A fleet that runs many "public TLS web service" workloads writes its own helper
*on top of* `Workload` — this is layer (c), NOT part of the shipped library. The
ADR shows it only to demonstrate the surface:

```elm
-- user-land, in a fleet's own module — NOT Workload.emet
webService : String -> Image -> Workload
webService name img =
  Workload
    { name = name, image = img, env = []
    , ports = [ tcp 443 443, tcp 80 80 ]
    , volumes = [], restart = Always, expose = Public
    }
```

This is exactly the lichess-style shortcut the old `Service`/`Ingress` tried to
be — but now it is a thin function over the strongly-typed library, owned by the
fleet, not a golemd-shipped abstraction. Different fleets grow different (c)
helpers; the shipped surface is only (b).

### 10. Re-expressing the dogfoods; replacing the lichess shapes

**Recommend replacing** the lichess `Workload`/`Service`/`Ingress`. The new
`Workload` subsumes all three: the old `Workload` is `Workload` with empty
lists + `Unexposed`; `Service` is `expose = Internal`; `Ingress`'s *firewall*
half is `expose = Public` and its *nginx* half is a separate proxy-container
concern (§7). The one type carries the ports/volumes/env the old three lacked.

The **registry** re-expressed:

```elm
registry : Workload
registry = Workload
  { name = "registry"
  , image = image "docker.io/library/registry:2"
  , env = []
  , ports = [ tcp 5000 5000 ]
  , volumes =
      [ FromVolume { volume = "golem-registry-data", at = "/var/lib/registry"
                   , access = ReadWrite, relabel = Private } ]  -- :Z
  , restart = Always
  , expose = Internal
  }
```

`workloadGlyphs registry` yields the podman apt + the `golem-registry-data.volume`
unit + the `.container` quadlet (`PublishPort=5000:5000/tcp`,
`Volume=golem-registry-data.volume:/var/lib/registry:Z`, `Restart=always`) + the
service, **plus** the internal firewall opening the hand-rolled example lacked —
no `String.join` in the fleet, and the named volume is now a real `.volume` unit
rather than a bare string. The **website Caddy** container is the same shape with
`expose = Public`, TLS ports, and a `FromHost` mount for its config (whose
`directory` source glyph is emitted for free). The insecure-registry drop-in on
*clients* (`clients.emet`) configures the podman *client*, not a running
container, and stays a plain `file` — this library does not absorb it.

## Alternatives considered

1. **A generic `Container` record (the first draft).** One `Container { image :
   String, ports, volumes, env, restart, expose }` where a volume is a field of
   the container. **Rejected on Dr. Dub's review**: it invents an abstraction
   instead of modeling the real artifact. Podman does not have "a container with
   volumes" — it has a `.container` unit that *references* separate `.volume`
   units by a mount line. Modeling volumes as a container field hides that a named
   volume is its own quadlet and blurs the `FromVolume`/`FromHost` distinction
   that decides whether a `directory` glyph is emitted. It also left `image` a
   `String`. This ADR models the quadlets themselves.

2. **A thin flat record of optionals** (`{ image, port : Maybe Int, volume :
   Maybe String, source : Maybe String, readOnly : Bool, relabel : Maybe String
   }`). Rejected — the illegal-states trap ADR 0019 already rejected for the
   filesystem entry. `port : Maybe Int` allows one port; `volume` + `source`
   optionals make "named volume with a host source" and "host path with no source"
   representable; `relabel : Maybe String` re-admits `":garbage"`. The `Mount` sum
   pushes each field onto the one arm that gives it meaning.

3. **A golemd-side `container` glyph kind** (a fifth reconciler that takes
   `image`/`ports`/`volumes` and runs podman itself). Rejected outright — it
   violates the four-glyph model (root `CLAUDE.md`, ADR 0002): golemd would grow
   podman/quadlet knowledge, a new `key()` namespace, a new `Inverse`, and a
   `format_version` bump, to express what composes cleanly from the four glyphs it
   already reverses. Quadlets are a *library* abstraction that compiles down, the
   same line ADR 0019 held for directories.

4. **`env : List (String, String)`.** Rejected mechanically: Emet has no tuple
   type (Context). `EnvVar { name, value }` is the representable and more readable
   form.

5. **Keep `Ingress` as the nginx front door and only add ports/volumes/env to the
   old `Workload`.** Rejected as half the fix: it leaves the port/firewall drift
   in place and keeps three shapes where one `Workload` with an `Expose` field is
   clearer. The reverse-proxy *config* genuinely is a separate follow-on, but the
   *firewall exposure* belongs on the workload that owns the ports.

## Consequences

- **golem ships a strongly-typed model of Podman Quadlets**, not an invented
  container abstraction. `ContainerUnit` and `VolumeUnit` mirror the `.container`
  and `.volume` files on disk; `Workload` (with `env`) is the ergonomic surface
  over them. The registry and website dogfoods re-express without hand-written
  `String.join` quadlets, and gain the firewall opening (registry) and host-path
  source directory (website) they lacked or hand-rolled.
- **Illegal states are unrepresentable** (Dr. Dub's review ask): `image` is an
  `Image` with a tag-xor-digest `Ref`; a mount's named-vs-host distinction is a
  `Mount` arm (no stray `source`, and the `.volume`-unit-vs-`directory`-glyph
  decision follows the arm); `:Z`/`:ro` are closed enums; `restart`/`proto` are
  enums; firewall exposure is *derived from* the published ports.
- **The three layers are explicit**: the four glyphs (a) are reused untouched;
  the shipped quadlet library (b) is this ADR's deliverable; lichess-style
  shortcuts (c) are user-land Emet on top of (b), demonstrated but not shipped.
- **Scope is bounded on purpose**: Debian + systemd + Podman-Quadlet only. A
  different runtime/OS is a different library targeting the same four glyphs — out
  of scope, a later design if ever needed.
- **`directory` (ADR 0019) gets its first library consumer**: a `FromHost` mount
  emits its bind-mount source directory automatically, closing the podman
  `statfs`-refusal gap without the author remembering the glyph.
- **No golemd change and no `format_version` bump.** Entirely an Emet library
  above the glyph layer; the wire contract (ADR 0012/0013) is untouched. golem
  still owns exactly the four glyphs.
- **Replacing `Workload`/`Service`/`Ingress` is a source migration** of
  `examples/lichess/` and the registry/website examples — no compiler work, but a
  breaking change to any fleet importing the old shapes (dogfoods, so the cost is
  a re-author, per the ADR 0019 disposable-fleet stance).
- **Forecloses** growing container capability by widening a flat record of
  optionals; commits the library to "model the actual quadlet unit kinds, each
  field a real quadlet key." Adding a unit kind later (`.network`, `.pod`) is a
  new `*Unit` type of its own; adding a container concern (healthcheck, cpu/memory
  limits) is a new typed field on `ContainerUnit`.
- **Cross-references:** composes the four glyphs of ADR 0002 and the `directory`
  arm of ADR 0019; authored with the parameterized ADTs of ADR 0016 and the
  exhaustive `case` of ADR 0005; supersedes the lichess `Workload`/`Service`/
  `Ingress` of `examples/lichess/Lichess.emet`.

## Open decisions for review (Dr. Dub)

- **How much of the quadlet surface to model now vs. incrementally.**
  `ContainerUnit` models `Image`/`Exec`/`PublishPort`/`Volume`/`Environment`/
  `Restart`/`WantedBy`. Real quadlets have dozens more keys (`Network`,
  `HealthCmd`, `PodmanArgs`, `Label`, `Secret`, …). Model the common set now and
  grow field-by-field, or aim for coverage up front? (Recommendation: the set
  above now; each further key is a typed field when a dogfood needs it.)
- **Are `.network`/`.pod` in scope now?** They are named as future unit kinds.
  Model them as sibling `*Unit` types this round, or defer until a dogfood needs
  inter-container networking? (Recommendation: defer; `.container` + `.volume`
  cover every current dogfood.)
- **`Image` tag-vs-digest shape.** Full parse/validation of
  `registry/name:tag` and `name@algo:hex` in the smart constructor, or a lightly
  validated 3-field record with a `Ref` sum and trust the author for the hex?
  (Recommendation: `Ref` sum now, light validation; tighten the digest grammar
  only if it bites.)
- **`env` as `EnvVar` records** (Emet has no tuple type). Confirm the named-record
  form is acceptable, or is a future tuple type wanted in the language instead?
  (Recommendation: `EnvVar` records; reads better regardless.)
- **`volumeUnits` derived vs. explicit** on `ContainerUnit`. Derive them from the
  `FromVolume` mounts (one `VolumeUnit` per referenced name, default driver), or
  carry them explicitly so a `.volume` can set a non-`local` driver / options?
  (Recommendation: derive by default in `Workload`; keep `ContainerUnit`'s field
  explicit for the driver-override case.)
- **Firewall `Expose` granularity.** Is whole-workload `Unexposed | Internal |
  Public` enough, or do we need per-port exposure (some ports internal, some
  public on one container)? (Recommendation: whole-workload now; promote to
  `Port`-level only if a dogfood needs it — Emet's lack of tuples makes a
  `List (Port, Expose)` a record anyway.)
- **Reverse-proxy config placement.** The nginx/Caddy *site* config is left out
  (a proxy is itself a `Workload`). Wanted as a follow-on `proxy`/`site` helper
  (layer c), or should `Workload` carry an optional `proxyFor` and emit a
  Caddyfile? (Recommendation: separate follow-on / user-land helper — keep the
  library about the runtime, not proxy templating.)
