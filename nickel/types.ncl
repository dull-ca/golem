# types.ncl — building blocks attached to Hosts.
#
# Two shapes do all the work:
#
#   Workload — the container. Image, env, volumes, command. Runs on a host.
#             May expose any number of ports internally. Has no idea who's
#             allowed to talk to it.
#
#   Service  — internal port routing. Names a (workload, port) pair, with
#             an `allow_from` firewall surface. The thing Ingress backends
#             reference, the thing other services and workloads dial.
#
# Mongo, Postgres, Redis, Fishnet, lila, and friends are all just Workloads
# with a particular image and env. If they accept connections, they get one
# or more Service entries pointing at the right ports. If they only connect
# out (a worker, a migration), they're a Workload with no Service.
#
# Templates are plain records. Per-host instances are made with `&`:
#
#   let lila = { image = "...", env = {...} } in
#   workloads = [ lila & { name = "lila" } ],
#   services  = [ { name = "lila", workload = "lila", port = 9663,
#                   allow_from = ["edge-01"] } ],

{
  # ── Volume ─────────────────────────────────────────────────────────────
  Volume = {
    at   | String,
    size | String | optional,
  },

  # ── Config ─────────────────────────────────────────────────────────────
  # Inline config file rendered into the container at a known path.
  Config = {
    path    | String,
    content | String,
    mode    | Number | default = 420,
  },

  # ── Workload ───────────────────────────────────────────────────────────
  # The container. Anything that runs — web service, mongo replica, fishnet
  # worker, postgres, the migrate oneshot — is a Workload. The schema
  # doesn't distinguish "stateful" from "stateless", "data" from "app".
  # That's the user's pattern in their templates.
  Workload = {
    name      | String,
    image     | String,
    command   | String              | optional,
    user      | String              | optional,
    env       | { _ : String }      | default = {},
    volume    | Volume              | optional,
    volumes   | { _ : Volume }      | default = {},
    configs   | Array Config        | default = [],
    digest    | String              | optional,
    restart   | [| 'always, 'on-failure, 'no' |] | default = 'always',
    oneshot   | Bool                | default = false,

    # Lifecycle flags — for migration paths. `managed = false` means golem
    # records the workload exists but won't touch it (so the schema and
    # the box can match without golem yanking a hand-installed thing).
    managed   | Bool                | default = true,
    disabled  | Bool                | default = false,
  },

  # ── Service ────────────────────────────────────────────────────────────
  # Internal port routing. A name + a (workload, port) pair + an
  # `allow_from` firewall surface. Other workloads and ingress backends
  # reference this by name.
  #
  # Multiple Services can target the same Workload on different ports —
  # e.g. one Workload exposes 8080 (web) and 9090 (metrics), with two
  # Services carrying different `allow_from` lists.
  #
  # `allow_from` is a list of *hostnames* (Inventory host keys) or group
  # names (from Inventory.groups, including tag-derived membership). The
  # translator emits an nftables fragment on this host. Default is
  # **deny everything except localhost** — opt in to who reaches you.
  Service = {
    name        | String,
    workload    | String,                          # name of a Workload on this host
    port        | Number,                          # which port on the workload
    allow_from  | Array String   | default = [],
  },

  # ── Ingress ────────────────────────────────────────────────────────────
  # Public HTTPS ingress (nginx). `backend` names a Service — the
  # translator resolves it to the host carrying that Service and
  # reverse-proxies across the private network.
  #
  # `dns = true` makes golem own the DNS record for `host` (an A/AAAA at
  # the ingress host's public addresses).
  Ingress = {
    host                  | String,
    backend               | String,                # name of a Service (any host)
    tls                   | [| 'auto, 'none |] | default = 'auto,

    dns                   | Bool   | default = false,

    # Rate limiting (nginx limit_req).
    rate_limit            | String | optional,     # "100r/m", "5r/s"
    rate_limit_burst      | Number | optional,
    rate_limit_zone_size  | String | optional,

    # Connection limiting (nginx limit_conn).
    conn_limit            | Number | optional,
    conn_limit_zone_size  | String | optional,

    # nginx low-level knobs.
    backlog               | Number | optional,
    client_body_timeout   | String | optional,
    client_header_timeout | String | optional,
    cors                  | Bool   | default = false,
    buffering             | Bool   | default = true,

    # Path-level routing. When non-empty, paths are dispatched to
    # different Services; the top-level `backend` is the fall-through.
    routes                | Array { path | String, backend | String } | default = [],

    # Free-form nginx site-block fragment for things the schema doesn't
    # express. Try not to use this — common things should grow a field.
    raw                   | String | optional,
  },
}
