# examples/simple/config.ncl
#
# A small lichess-shaped slice across three hosts: an edge box terminating
# public ingress, an app box running lila + a fishnet worker pool, and a
# data box running mongo and postgres. Demonstrates:
#
#   - Workload (the container) + Service (port routing with firewall) split
#   - Mongo, postgres, fishnet all expressed as plain Workload templates
#     in user-land — nothing about them is in the schema
#   - `allow_from` referencing other host names AND a group
#   - First-class DNS (`dns = true` on Ingress)
#   - Groups derived from `tags` on hosts
#
# `golemctl apply` walks this Inventory, computes one Bundle per host, and
# pushes each over to the agent at /apply on that host.

let g = import "../../nickel/lib.ncl" in

# ── Workload templates ────────────────────────────────────────────────────
# These are plain records. Per-host instances are made with `&`. In a real
# repo they'd live in a `workloads/` directory imported here.

let lila = {
  image = "ghcr.io/lichess-org/lila:latest",
  env   = {
    LILA_DB_URI = "mongodb://%{g.ref.service "mongo-lichess"}/lichess",
    LILA_REDIS  = "%{g.ref.service "redis"}",
    JAVA_OPTS   = "-Xmx12g -XX:+UseG1GC",
  },
} in

let lila_ws = {
  image = "ghcr.io/lichess-org/lila-ws:latest",
  env   = {
    LILA_WS_REDIS = "%{g.ref.service "redis"}",
  },
} in

let mongo = fun instance => {
  name   = "mongo-%{instance}",
  image  = "mongo:7",
  volume = { at = "/data/db", size = "200Gb" },
} in

let postgres = fun version => {
  image  = "postgres:%{version}",
  volume = { at = "/var/lib/postgresql/data", size = "50Gb" },
} in

let redis = {
  image  = "redis:7-alpine",
  volume = { at = "/data", size = "5Gb" },
} in

let fishnet = {
  image = "ghcr.io/lichess-org/fishnet:latest",
  env   = {
    FISHNET_KEY    = g.ref.secret "fishnet_key",
    FISHNET_CORES  = "4",
    FISHNET_TARGET = "https://lichess.example.com",
  },
} in

# ── Inventory ─────────────────────────────────────────────────────────────
{
  hosts = {
    # Edge box: public-facing nginx, owns DNS for the zone.
    "edge-01" = {
      name      = "edge-01",
      public_ip = "203.0.113.10",
      vrack_ip  = "10.42.0.1",
      tags      = ["dns-primary"],

      ingress = [
        {
          host                 = "lichess.example.com",
          backend              = "lila",
          dns                  = true,
          rate_limit           = "100r/m",
          rate_limit_burst     = 20,
          rate_limit_zone_size = "10m",
          conn_limit           = 50,
          buffering            = false,
          routes = [
            { path = "/socket",  backend = "lila-ws" },
            { path = "/socket/", backend = "lila-ws" },
          ],
        },
      ],
    },

    # App box: lila web + lila-ws + a fishnet worker pool.
    "app-01" = {
      name      = "app-01",
      public_ip = "203.0.113.20",
      vrack_ip  = "10.42.0.2",
      tags      = ["mongodb-clients", "redis-clients"],

      workloads = [
        lila    & { name = "lila" },
        lila_ws & { name = "lila-ws" },
        # Fishnet is a Workload only — it dials out, nothing dials in.
        fishnet & { name = "fishnet-1" },
        fishnet & { name = "fishnet-2" },
      ],

      services = [
        { name = "lila",    workload = "lila",    port = 9663,
          allow_from = ["edge-01"] },
        { name = "lila-ws", workload = "lila-ws", port = 9664,
          allow_from = ["edge-01"] },
      ],
    },

    # Data box: mongo + postgres + redis. Each is a Workload with a
    # matching Service that carries the firewall surface.
    "db-01" = {
      name      = "db-01",
      public_ip = "203.0.113.30",
      vrack_ip  = "10.42.0.3",

      workloads = [
        mongo "lichess",

        postgres "18" & {
          name = "analytics",
          env  = {
            POSTGRES_USER     = "app",
            POSTGRES_PASSWORD = g.ref.secret "pg_password",
            POSTGRES_DB       = "analytics",
          },
        },

        redis & { name = "redis" },
      ],

      services = [
        { name = "mongo-lichess", workload = "mongo-lichess", port = 27017,
          allow_from = ["mongodb-clients"] },        # tag membership: app-01

        { name = "analytics", workload = "analytics", port = 5432,
          allow_from = ["app-01"] },

        { name = "redis", workload = "redis", port = 6379,
          allow_from = ["redis-clients"] },          # tag membership: app-01
      ],
    },
  },

  # Groups can be listed explicitly; membership is the union of the list
  # here and any host whose `tags` contains the group name.
  groups = {},
} | g.Inventory
