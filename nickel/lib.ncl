# lib.ncl — the public surface for users.
#
# A user's config.ncl imports this. It provides:
#
#   1. Primitives — small building blocks (file, package, container, …)
#   2. Composition — combine, own, bundle, bundle_for
#   3. Contrib — shipped higher-level abstractions (workload, postgres, …)
#   4. Ref helpers — cross-host placeholder resolution
#   5. Type contracts — Workload, Service, Ingress, etc. (for users who
#      want to use them as contracts on their own abstractions)

let t      = import "types.ncl" in
let h      = import "host.ncl" in
let i      = import "inventory.ncl" in
let frag   = import "fragment.ncl" in
let prim   = import "primitives.ncl" in
let bun    = import "bundle.ncl" in
let resolve = import "resolve.ncl" in
let contrib_mod = import "contrib.ncl" in

{
  # ── Primitives ───────────────────────────────────────────────────────
  # The small building blocks. Each takes a config record and returns a
  # Fragment. Compose them with `combine`.
  #
  #   g.file { path = "/etc/foo", content = "bar" }
  #   g.package { name = "nginx" }
  #   g.container { name = "lila", image = "...", env = {...} }
  #   g.firewall { name = "lila", port = 9663, allow = ["10.42.0.1"] }
  #
  file       = prim.file,
  package    = prim.package,
  unit       = prim.unit,
  container  = prim.container,
  firewall   = prim.firewall,
  dns_record = prim.dns_record,
  site       = prim.site,

  # ── Composition ──────────────────────────────────────────────────────
  # Merge fragments, tag ownership, assemble bundles.
  #
  #   g.combine [ g.package { name = "podman" }, g.container { ... } ]
  #   g.own "workload/lila" (g.combine [ ... ])
  #   g.bundle "app-01" [ ... ]
  #
  Fragment   = frag.Fragment,
  combine    = frag.combine,
  own        = frag.own,
  bundle     = bun.bundle,
  bundle_for = bun.bundle_for,
  node_names = bun.node_names,

  # ── Resolve ──────────────────────────────────────────────────────────
  # Cross-host reference resolution. Used internally by bundle_for, but
  # exposed for users who want to resolve manually.
  resolve_allow_from = resolve.resolve_allow_from,
  resolve_string     = resolve.resolve_string,

  # ── Contrib ──────────────────────────────────────────────────────────
  # Shipped higher-level abstractions. Use them directly or copy the
  # pattern to build your own.
  #
  #   g.contrib.workload { name = "lila", image = "..." }
  #   g.contrib.postgres { name = "appdb", db = "litour", ... }
  #   g.contrib.worker { name = "celery", image = "..." }
  #
  contrib = contrib_mod,

  # ── Placeholder helpers ──────────────────────────────────────────────
  # These emit literal placeholder strings that bundle_for substitutes
  # at translation time. Use them in env values and config files for
  # cross-workload references.
  #
  #   env = { LILA_DB = "mongodb://%{g.ref.service "mongo-lichess"}/lichess" }
  #
  ref = {
    service = fun name => "{%{name}.service}",
    host    = fun name => "{%{name}.host}",
    secret  = fun name => "{secret:%{name}}",
  },

  # ── Type contracts ───────────────────────────────────────────────────
  # These are the high-level shapes from the original schema. Users can
  # use them as contracts on their own abstractions, or ignore them
  # entirely and work directly with primitives.
  Inventory = i.Inventory,
  Host      = h.Host,
  Role      = h.Role,
  RoleDeps  = h.RoleDeps,
  Workload  = t.Workload,
  Service   = t.Service,
  Ingress   = t.Ingress,
  Volume    = t.Volume,
  Config    = t.Config,
}
