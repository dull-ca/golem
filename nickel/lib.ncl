# lib.ncl — top-level Fleet contract and per-node bundle extraction.
#
# A user's config.ncl imports this and produces a Fleet. `golemctl` then
# calls `bundle_for` with a node name to get the bundle for that node.

let c = import "claims.ncl" in
let w = import "workload.ncl" in
let i = import "ingress.ncl" in

{
  Node = {
    name    | String,
    roles   | Array String | default = [],     # "edge", "app", "db", ...
    address | String,                          # nebula or lan IP
  },

  Fleet = {
    version   | Number,
    nodes     | { _ : Node },
    workloads | { _ : w.Workload } | default = {},
    ingresses | { _ : i.Ingress }  | default = {},

    # ── derived helpers ────────────────────────────────────────────────────
    node_list = std.record.values nodes,

    edge_nodes =
      node_list
      |> std.array.filter (fun n => std.array.elem "edge" n.roles)
      |> std.array.map    (fun n => n.name),

    # Resolve "workload name" -> "host:port" for the Ingress site block.
    # Picks the first node running the workload + its first published port.
    # A real implementation would load-balance; for M1 this is fine.
    upstream_resolver = fun wname =>
      let wl = workloads."%{wname}" in
      let node_name = std.array.first wl.nodes in
      let addr = nodes."%{node_name}".address in
      let port = (std.array.first wl.ports).host in
      "%{addr}:%{std.to_string port}",

    # ── bundle_for: the thing golemctl calls ───────────────────────────────
    bundle_for = fun node_name =>
      let _ = std.contract.apply (std.contract.from_predicate
                (fun _ => std.record.has_field node_name nodes))
              "unknown node: %{node_name}" null in
      let workload_claims =
        workloads
        |> std.record.values
        |> std.array.map (fun wl => w.workload_claims_for wl node_name)
        |> std.array.flatten in
      let ingress_claims =
        ingresses
        |> std.record.values
        |> std.array.map (fun ig => i.ingress_claims_for ig node_name edge_nodes upstream_resolver)
        |> std.array.flatten in
      let ingress_handlers =
        ingresses
        |> std.record.values
        |> std.array.map (fun ig => i.ingress_handler_for ig node_name edge_nodes)
        |> std.array.flatten in
      {
        version = version,
        node = node_name,
        claims = workload_claims @ ingress_claims,
        handlers = ingress_handlers,
      } | c.Bundle,
  },

  # Re-export so user configs can do `let g = import "lib.ncl" in g.Workload`.
  Workload = w.Workload,
  Ingress  = i.Ingress,
}
