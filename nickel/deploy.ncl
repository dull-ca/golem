# deploy.ncl — the Deploy contract.
#
# A Deploy says "run this app on these nodes, and expose these services to
# the world at these hostnames." It's the placement + ingress story. The App
# itself doesn't know about nodes or public hostnames; that's intentional —
# the same App deploys to staging and prod by changing only this file.
#
# A Deploy can also include raw layer-3 escape-hatch claims (extra files,
# packages, units) for things that don't fit the App model — e.g. running
# postgres as an apt package on the host instead of as a container.

let a = import "app.ncl" in

{
  # ── Node ───────────────────────────────────────────────────────────────
  Node = {
    name    | String,
    address | String,                           # nebula or LAN address
  },

  # ── Expose ─────────────────────────────────────────────────────────────
  # Public ingress: hostname → service. Caddy fronts everything; TLS is
  # automatic via Let's Encrypt unless `tls = 'none`.
  Expose = {
    host    | String,                           # "hello.example.com"
    service | String,                           # service name in the App
    tls     | [| 'auto, 'none |] | default = 'auto,
    # Optional path-level routing for multi-service hosts (e.g. /v2/api → api,
    # /* → web). When omitted, `service` handles everything for `host`.
    routes  | Array { path | String, service | String } | default = [],
  },

  # ── SystemEscape ───────────────────────────────────────────────────────
  # Layer-3 claims declared inline at deploy time. Use sparingly — most
  # things should live in the App. This is for OS-level concerns the App
  # model can't express (kernel sysctls, custom systemd timers, etc.).
  #
  # The record key is the runtime name in every case:
  #   packages.<package-name>           — apt package name
  #   files."<key>"                     — free-form id; the actual filesystem
  #                                       path is the `path` field
  #   units."<unit-name>.service"       — systemd unit name (key includes
  #                                       the `.service` suffix)
  SystemEscape = {
    packages | { _ : {} }                                                          | default = {},
    files    | { _ : { path | String, content | String, mode | Number | default = 420 } } | default = {},
    units    | { _ : { enable | Bool | default = true, active | Bool | default = true } } | default = {},
  },

  # ── Deploy ─────────────────────────────────────────────────────────────
  # Entry point. `app` is the App to run, `nodes` is the list of boxes,
  # `place` decides which services run where (defaults to "all on the one
  # node" if there's only one), `expose` lists public hostnames.
  Deploy = {
    version | Number,
    app     | a.App,
    nodes   | { _ : Node },
    # placement: service-name -> [node-name]. Missing services default to
    # "all nodes" if there's exactly one node, otherwise it's an error.
    place   | { _ : Array String } | default = {},
    expose  | Array Expose         | default = [],
    system  | SystemEscape         | default = { packages = {}, files = {}, units = {} },
  },
}
