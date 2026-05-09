# examples/simple/config.ncl
#
# A small lichess-shaped slice across three hosts. Demonstrates:
#
#   - Primitives (file, package, container, firewall) composed into
#     higher-level abstractions via contrib
#   - Shipped contrib constructors (postgres, redis, mongo)
#   - Cross-host reference resolution via g.ref.service / g.ref.secret
#   - DNS record management via contrib.ingress

let g = import "../../nickel/lib.ncl" in

# ── Host addresses ────────────────────────────────────────────────────
# Defined up front so fragments can reference other hosts' IPs.
let edge_ip  = "10.42.0.1" in
let app_ip   = "10.42.0.2" in
let db_ip    = "10.42.0.3" in

# ── Shared env ────────────────────────────────────────────────────────
let lila_env = {
  LILA_DB_URI = "mongodb://%{g.ref.service "mongo-lichess"}/lichess",
  LILA_REDIS  = "%{g.ref.service "redis"}",
  JAVA_OPTS   = "-Xmx12g -XX:+UseG1GC",
} in

let fishnet_env = {
  FISHNET_KEY    = g.ref.secret "fishnet_key",
  FISHNET_CORES  = "4",
  FISHNET_TARGET = "https://lichess.example.com",
} in

# ── Inventory ─────────────────────────────────────────────────────────
let inventory = {
  hosts = {
    # ── Edge box: public-facing nginx, owns DNS for the zone. ────────
    "edge-01" = {
      name      = "edge-01",
      public_ip = "203.0.113.10",
      vrack_ip  = edge_ip,
      tags      = ["dns-primary"],

      fragments = [
        g.contrib.ingress {
          host                 = "lichess.example.com",
          upstream             = "%{g.ref.service "lila"}",
          dns                  = true,
          dns_ip               = "203.0.113.10",
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

    # ── App box: lila + lila-ws + fishnet workers. ────────────────────
    "app-01" = {
      name      = "app-01",
      public_ip = "203.0.113.20",
      vrack_ip  = app_ip,
      tags      = ["mongodb-clients", "redis-clients"],

      fragments = [
        # lila web — exposed on port 9663, reachable from edge-01
        g.contrib.web_service {
          name  = "lila",
          image = "ghcr.io/lichess-org/lila:latest",
          port  = 9663,
          allow = [edge_ip],
          env   = lila_env,
        },

        # lila-ws — websocket service
        g.contrib.web_service {
          name  = "lila-ws",
          image = "ghcr.io/lichess-org/lila-ws:latest",
          port  = 9664,
          allow = [edge_ip],
          env   = { LILA_WS_REDIS = "%{g.ref.service "redis"}" },
        },

        # fishnet — workers that dial out only, no inbound ports
        g.contrib.worker {
          name  = "fishnet-1",
          image = "ghcr.io/lichess-org/fishnet:latest",
          env   = fishnet_env,
        },
        g.contrib.worker {
          name  = "fishnet-2",
          image = "ghcr.io/lichess-org/fishnet:latest",
          env   = fishnet_env,
        },
      ],

      # Services: declares which service names live on this host and at
      # what port. Used by g.ref.service for cross-host resolution.
      services = [
        { name = "lila",    port = 9663 },
        { name = "lila-ws", port = 9664 },
      ],
    },

    # ── Data box: mongo + postgres + redis. ───────────────────────────
    "db-01" = {
      name      = "db-01",
      public_ip = "203.0.113.30",
      vrack_ip  = db_ip,

      fragments = [
        g.contrib.mongo {
          name  = "mongo-lichess",
          allow = [app_ip],
        },

        g.contrib.postgres {
          name     = "analytics",
          db       = "analytics",
          user     = "app",
          password = g.ref.secret "pg_password",
          allow    = [app_ip],
        },

        g.contrib.redis {
          name  = "redis",
          allow = [app_ip],
        },
      ],

      services = [
        { name = "mongo-lichess", port = 27017 },
        { name = "analytics",     port = 5432 },
        { name = "redis",         port = 6379 },
      ],
    },
  },

  groups = {},
} in

# ── Export ────────────────────────────────────────────────────────────
# golemctl calls `.bundle_for "node-name"` to get the bundle for a host.
{
  bundle_for = g.bundle_for inventory,
  nodes      = inventory.hosts,
}
