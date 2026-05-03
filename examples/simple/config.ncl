# examples/simple/config.ncl
#
# A two-node fleet: one edge (caddy + public HTTPS), one app (runs the api
# container). This is the DX we're optimizing for — simple things are short.

let g = import "../../nickel/lib.ncl" in

{
  version = 1,

  nodes = {
    "edge-01" = { name = "edge-01", roles = ["edge"], address = "10.42.0.1" },
    "app-01"  = { name = "app-01",  roles = ["app"],  address = "10.42.0.2" },
  },

  workloads = {
    api = {
      name  = "api",
      image = "ghcr.io/example/api:v1.2.3",
      nodes = ["app-01"],
      ports = [{ host = 8080, container = 8080 }],
      env   = { DATABASE_URL = "postgres://db.internal/prod" },
    },
  },

  ingresses = {
    "api.example.com" = {
      host     = "api.example.com",
      workload = "api",
    },
  },
} | g.Fleet
