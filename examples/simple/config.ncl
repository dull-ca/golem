# examples/simple/config.ncl
#
# A small Django app + postgres + caddy on a single box. Demonstrates the
# two-layer input model:
#
#   1. `app` describes what's running and how the services wire together.
#   2. `deploy` says where to put it and how the world reaches it.

let g = import "../../nickel/lib.ncl" in

{
  app.services = {
    postgres = {
      image  = "ghcr.io/postgresql/postgresql-18:latest",
      volume = { at = "/var/lib/postgresql/data", size = "5Gb" },
      env    = {
        POSTGRES_USER     = "app",
        POSTGRES_PASSWORD = "dev-change-me",
        POSTGRES_DB       = "app",
      },
    },

    web = {
      image = "ghcr.io/foobarorg/foobar:latest",
      env   = {
        DATABASE_URL = "postgres://app:dev-change-me@%{g.ref.service "postgres"}:5432/app",
      },
    },

    caddy = {
      image  = "caddy:2",
      config = {
        path    = "/etc/caddy/Caddyfile",
        content = m%"
          hello.example.com {
            reverse_proxy %{g.ref.port "web" 8000}
          }
        "%,
      },
    },
  },

  nodes = {
    "box-01" = { name = "box-01", address = "10.42.0.1" },
  },

  expose = [
    { host = "hello.example.com", service = "caddy" },
  ],
} | g.Deploy
