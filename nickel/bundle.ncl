# bundle.ncl — bundle assembly.
#
# The `bundle` function takes a node name and an array of fragments and
# produces a Bundle in the wire format the agent expects.
#
# The `bundle_for` function is the high-level entry point: it takes an
# inventory-shaped config, extracts the fragments for a given node,
# resolves cross-host references, and produces a signed-ready Bundle.

let f = import "fragment.ncl" in
let res = import "resolve.ncl" in

{
  # ── bundle ─────────────────────────────────────────────────────────────
  # Assemble fragments into a Bundle. Auto-assigns ownership based on
  # resource naming if owners are empty.
  #
  #   bundle "app-01" [
  #     g.package { name = "podman" },
  #     g.container { name = "lila", image = "..." },
  #   ]
  #
  bundle = fun node fragments =>
    let combined = f.combine fragments in
    let resources_with_owners =
      combined.resources
      |> std.array.map (fun r =>
        if std.array.length r.owners > 0 then r
        else r & { owners = ["golem/%{r.id.key}"] })
    in
    {
      version  = 0,
      node     = node,
      claims   = resources_with_owners,
      handlers = combined.handlers,
    },

  # ── bundle_for ─────────────────────────────────────────────────────────
  # High-level entry point. Takes an inventory and produces a bundle_for
  # function that golemctl can call.
  #
  # The inventory must have:
  #   hosts: { host_name: { name, ip or vrack_ip or public_ip,
  #                         fragments: [...] } }
  #
  # Each host's `fragments` is an array of Fragments (from primitives or
  # composed higher-level abstractions).
  #
  # Usage in config.ncl:
  #
  #   let g = import "nickel/lib.ncl" in
  #   let inventory = {
  #     hosts = {
  #       "app-01" = {
  #         name = "app-01", vrack_ip = "10.42.0.2",
  #         fragments = [ g.container { ... }, g.firewall { ... } ],
  #       },
  #     },
  #   } in
  #   { bundle_for = g.bundle_for inventory }
  #
  bundle_for = fun inventory => fun node_name =>
    let _ =
      std.contract.apply
        (std.contract.from_predicate
          (fun _ => std.record.has_field node_name inventory.hosts))
        "unknown node: %{node_name}"
        null
    in
    let host = std.record.get node_name inventory.hosts in
    let fragments =
      if std.record.has_field "fragments" host
      then host.fragments
      else []
    in
    let combined = f.combine fragments in
    let resolved = res.resolve_fragment inventory combined in
    let resources_with_owners =
      resolved.resources
      |> std.array.map (fun r =>
        if std.array.length r.owners > 0 then r
        else r & { owners = ["golem/%{r.id.key}"] })
    in
    {
      version  = 0,
      node     = node_name,
      claims   = resources_with_owners,
      handlers = resolved.handlers,
    },

  # ── node_names ─────────────────────────────────────────────────────────
  # Extract the list of host names from an inventory. Used by golemctl
  # to iterate over all hosts.
  node_names = fun inventory =>
    std.record.fields inventory.hosts,
}
