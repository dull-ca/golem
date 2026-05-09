# host.ncl — the Host contract.
#
# A Host is one box in the inventory. It owns its addresses, the workloads
# that run on it, the services that route into those workloads, and any
# ingress it terminates.
#
# Workload templates are written as plain records (typically in a
# `workloads/` directory in the user's repo) and instantiated per-host
# with Nickel's record merge:
#
#   workloads = [
#     mongo "lichess",
#     postgres "18" & { name = "analytics", env = {...} },
#   ],
#
# Roles + role_deps live on the Inventory, not on the Host — they're
# cross-cutting bringup ordering concerns.

let t = import "types.ncl" in

{
  # ── Host ───────────────────────────────────────────────────────────────
  Host = {
    # Identity. `name` doubles as the SSH host alias and as the key used
    # in Inventory.hosts.
    name          | String,

    # Public addressing. `public_ip` and `ipv6` are how the world reaches
    # this host; `vrack_ip` / `vrack2_ip` are private network addresses
    # (lichess uses OVH's vRack); `vrack_mac` is required for vRack member
    # provisioning. Extras let a host with multiple public addresses hand
    # out specific ones to specific services / DNS records.
    public_ip     | String          | optional,
    ipv6          | String          | optional,
    ipv6_gateway  | String          | optional,
    vrack_ip      | String          | optional,
    vrack2_ip     | String          | optional,
    vrack_mac     | String          | optional,
    extra_ipv4    | Array String    | default = [],
    extra_ipv6    | Array String    | default = [],

    # The actual containers running on this host.
    workloads     | Array t.Workload | default = [],

    # Internal port routing into those workloads. Each Service names a
    # workload + port and carries the firewall surface for that port.
    services      | Array t.Service  | default = [],

    # Public HTTPS ingress terminated on this host.
    ingress       | Array t.Ingress  | default = [],

    # Cross-cutting tags. Membership in Inventory.groups is computed from
    # these; group names are referenced by Service.allow_from etc.
    tags          | Array String     | default = [],
  },

  # ── Role ───────────────────────────────────────────────────────────────
  # A named bringup role. Used inside Inventory.role_deps to express
  # ordering between role-shaped lifecycle hooks (e.g. "ingress-cleanup
  # must finish before nginx starts"). Most deploys leave this empty.
  Role = {
    name        | String,
    depends_on  | Array String | default = [],
  },

  # ── RoleDeps ───────────────────────────────────────────────────────────
  RoleDeps = {
    roles       | Array Role | default = [],
  },
}
