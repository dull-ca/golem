# inventory.ncl — the top-level Inventory contract.
#
# An Inventory is a collection of Hosts plus cross-cutting concerns:
#
#   - groups:    named sets of host names. Service.allow_from can reference
#                a group name instead of listing every host individually.
#                Groups can be enumerated explicitly OR derived from
#                Host.tags (a host with tag "mongodb-clients" is a member
#                of group "mongodb-clients" automatically).
#   - role_deps: ordering between bringup roles. Used by lichess-style
#                migrations; small deploys leave this empty.
#
# `golemctl apply` walks an Inventory, computes per-host bundles, and
# pushes one signed bundle per host. Cross-host references (allow_from,
# DNS A records) are resolved at apply time using the addresses on each
# Host.

let h = import "host.ncl" in

{
  Inventory = {
    # Hosts keyed by name. The name in the key must equal Host.name.
    hosts      | { _ : h.Host },

    # Explicit groups. Each group is a list of host names. Membership is
    # the union of explicit listing here and any host whose `tags`
    # contains the group name.
    groups     | { _ : Array String } | default = {},

    # Optional role-dependency graph for migration scripts.
    role_deps  | h.RoleDeps           | default = { roles = [] },
  },
}
