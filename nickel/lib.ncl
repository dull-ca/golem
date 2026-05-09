# lib.ncl — the public surface for users.
#
# A user's config.ncl imports this and writes an Inventory. `golemctl`
# walks the Inventory and emits one signed Bundle per Host.

let c = import "claims.ncl" in
let t = import "types.ncl" in
let h = import "host.ncl" in
let i = import "inventory.ncl" in

{
  # ── Top level ─────────────────────────────────────────────────────────
  Inventory = i.Inventory,
  Host      = h.Host,
  Role      = h.Role,
  RoleDeps  = h.RoleDeps,

  # ── Building blocks attached to a Host ────────────────────────────────
  Workload = t.Workload,
  Service  = t.Service,
  Ingress  = t.Ingress,
  Volume   = t.Volume,
  Config   = t.Config,

  # ── Placeholder helpers ───────────────────────────────────────────────
  # These emit literal placeholder strings that golemctl substitutes at
  # translation time. Use them in env values and config files for cross-
  # workload references.
  #
  #   env = { LILA_DB = "%{ref.service "mongo-lichess"}" },
  #
  # Equivalent to writing the string `"{mongo-lichess.service}"` directly,
  # but compiler-checked: misspell a service name and `golemctl` will
  # reject the inventory because the placeholder doesn't resolve.
  ref = {
    # Address of a Service, anywhere in the Inventory. Resolves to
    # "<host>:<port>" using the host that owns the Service and the port
    # the Service advertises.
    service = fun name => "{%{name}.service}",

    # Address of a Host, by name. Resolves to its primary private
    # address (vrack_ip if set, else public_ip). Use `service` instead
    # when you want a specific port — `host` is for raw addressing.
    host    = fun name => "{%{name}.host}",

    # Operator-supplied secret. Values stage on the operator's laptop
    # under ~/.golem/secrets/<inventory>/<name> and decrypt to a tmpfs
    # path on the agent. M3.
    secret  = fun name => "{secret:%{name}}",
  },
}
