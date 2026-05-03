# ingress.ncl — public HTTPS ingress via Caddy.
#
# An Ingress binds a hostname to a Workload (or to a static upstream). Caddy
# handles Let's Encrypt automatically, so we just need to:
#   1. ensure caddy is installed on edge nodes
#   2. write a site block into /etc/caddy/Caddyfile.d/<host>.caddy
#   3. ensure caddy.service is active, with a handler that reloads on file change
#
# Edge nodes are determined by Fleet.edge_nodes (nodes with role "edge").

let c = import "claims.ncl" in

{
  Ingress = {
    host     | String,                               # "api.example.com"
    workload | String | optional,                    # name of a Workload to front
    upstream | String | optional,                    # or a raw upstream "host:port"
    tls      | Bool   | default = true,              # LE-managed cert
    # Escape hatch: replace the generated site block entirely.
    site_block | String | optional,
  },

  render_site = fun ing upstream_addr =>
    if ing.site_block != null then ing.site_block else
    let tls_dir = if ing.tls then "" else "\n    tls internal" in
    m%"
      # Managed by golem. Do not edit.
      %{ing.host} {%{tls_dir}
          reverse_proxy %{upstream_addr}
      }
    "%,

  # Expand an Ingress into claims, but only for nodes that carry the "edge" role.
  # `upstream_resolver` is a function String -> String (workload-name -> host:port)
  # provided by the Fleet at expansion time, so ingresses don't need to know
  # how workloads are scheduled.
  ingress_claims_for = fun ing node edge_nodes upstream_resolver =>
    if !(std.array.elem node edge_nodes) then [] else
    let owner = "ingress/%{ing.host}" in
    let upstream_addr =
      if ing.upstream != null then ing.upstream
      else if ing.workload != null then upstream_resolver ing.workload
      else std.fail_with "Ingress %{ing.host} needs workload or upstream" in
    let site_path = "/etc/caddy/Caddyfile.d/%{ing.host}.caddy" in
    let file_id = { kind = 'File, key = site_path } in
    [
      {
        id = { kind = 'AptPackage, key = "caddy" },
        spec = 'AptPackage { name = "caddy" },
        owners = [owner],
        after = [],
      },
      {
        id = file_id,
        spec = 'File {
          path = site_path,
          content = render_site ing upstream_addr,
          mode = 420,
          marker = 'Owned,
        },
        owners = [owner],
        after = [{ kind = 'AptPackage, key = "caddy" }],
      },
      {
        id = { kind = 'SystemdUnit, key = "caddy.service" },
        spec = 'SystemdUnit { name = "caddy.service", active = true, enable = true },
        owners = [owner],
        after = [file_id],
      },
    ],

  # Handler: if any ingress site file changes, reload caddy afterwards.
  ingress_handler_for = fun ing node edge_nodes =>
    if !(std.array.elem node edge_nodes) then [] else
    [{
      source = { kind = 'File, key = "/etc/caddy/Caddyfile.d/%{ing.host}.caddy" },
      targets = ["caddy.service"],
    }],
}
