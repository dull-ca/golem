# contrib.ncl — shipped higher-level abstractions.
#
# These are examples of composing primitives. Users can import them
# directly or copy the pattern to build their own.
#
# Every function here returns a Fragment — compose them with g.combine.

let f = import "fragment.ncl" in
let p = import "primitives.ncl" in

{
  # ── workload ───────────────────────────────────────────────────────────
  # A container workload: ensures podman is installed + runs the container.
  #
  #   contrib.workload {
  #     name = "lila", image = "ghcr.io/lichess-org/lila:latest",
  #     env = { JAVA_OPTS = "-Xmx12g" },
  #   }
  #
  workload = fun config =>
    f.own "workload/%{config.name}" (
      f.combine [
        p.package { name = "podman" },
        p.container config,
      ]
    ),

  # ── web_service ────────────────────────────────────────────────────────
  # A workload that listens on a port with firewall rules.
  #
  # `allow` is a list of IP addresses (already resolved). If you're
  # working with an inventory, use `resolve_allow_from` first.
  #
  #   contrib.web_service {
  #     name = "lila", image = "...",
  #     port = 9663, allow = ["10.42.0.1"],
  #     env = { ... },
  #   }
  #
  web_service = fun config =>
    f.own "service/%{config.name}" (
      f.combine [
        workload config,
        p.firewall {
          name     = config.name,
          port     = config.port,
          allow    = config.allow,
          protocol =
            if std.record.has_field "protocol" config
            then config.protocol
            else "tcp",
        },
      ]
    ),

  # ── ingress ────────────────────────────────────────────────────────────
  # Public HTTPS termination via nginx. Installs nginx, writes a site
  # config, and optionally manages the DNS record.
  #
  #   contrib.ingress {
  #     host = "lichess.example.com",
  #     upstream = "10.42.0.2:9663",
  #     dns = true, dns_ip = "203.0.113.10",
  #   }
  #
  ingress = fun config =>
    let base = f.combine [
      p.package { name = "nginx" },
      p.unit { name = "nginx.service" },
      p.site config,
    ] in
    let with_dns =
      if std.record.has_field "dns" config && config.dns then
        let parts = std.string.split "." config.host in
        let zone_name =
          let len = std.array.length parts in
          if len >= 2 then
            "%{std.array.at (len - 2) parts}.%{std.array.at (len - 1) parts}"
          else config.host
        in
        f.combine [
          base,
          p.dns_record {
            zone  = zone_name,
            name  = config.host,
            type  = "A",
            value = config.dns_ip,
          },
        ]
      else base
    in
    f.own "ingress/%{config.host}" with_dns,

  # ── postgres ───────────────────────────────────────────────────────────
  # A postgres database. Opinionated defaults, override what you need.
  #
  #   contrib.postgres {
  #     name = "appdb",
  #     db = "litour", user = "litour",
  #     password = g.ref.secret "pg_pass",
  #     allow = ["10.42.0.2"],
  #   }
  #
  postgres = fun config =>
    let pg_version =
      if std.record.has_field "version" config
      then config.version
      else "17"
    in
    let vol_size =
      if std.record.has_field "volume_size" config
      then config.volume_size
      else "50Gb"
    in
    let env_extra =
      if std.record.has_field "extra_env" config
      then config.extra_env
      else {}
    in
    let pg_configs =
      if std.record.has_field "configs" config
      then config.configs
      else []
    in
    web_service {
      name  = config.name,
      image = "postgres:%{pg_version}",
      port  = 5432,
      allow = config.allow,
      volume = { at = "/var/lib/postgresql/data", size = vol_size },
      env   = {
        POSTGRES_USER     = config.user,
        POSTGRES_PASSWORD = config.password,
        POSTGRES_DB       = config.db,
      } & env_extra,
      configs = pg_configs,
    },

  # ── redis ──────────────────────────────────────────────────────────────
  # A redis instance.
  #
  #   contrib.redis { name = "redis", allow = ["10.42.0.2"] }
  #
  redis = fun config =>
    let redis_image =
      if std.record.has_field "image" config
      then config.image
      else "redis:7-alpine"
    in
    let vol_size =
      if std.record.has_field "volume_size" config
      then config.volume_size
      else "5Gb"
    in
    web_service {
      name  = config.name,
      image = redis_image,
      port  = 6379,
      allow = config.allow,
      volume = { at = "/data", size = vol_size },
    },

  # ── mongo ──────────────────────────────────────────────────────────────
  # A MongoDB instance.
  #
  #   contrib.mongo { name = "mongo-lichess", allow = ["10.42.0.2"] }
  #
  mongo = fun config =>
    let mongo_image =
      if std.record.has_field "image" config
      then config.image
      else "mongo:7"
    in
    let vol_size =
      if std.record.has_field "volume_size" config
      then config.volume_size
      else "200Gb"
    in
    web_service {
      name  = config.name,
      image = mongo_image,
      port  = 27017,
      allow = config.allow,
      volume = { at = "/data/db", size = vol_size },
    },

  # ── worker ─────────────────────────────────────────────────────────────
  # A background worker — runs a container but doesn't expose any ports.
  # Nothing dials in; it only dials out.
  #
  #   contrib.worker { name = "celery", image = "...", env = { ... } }
  #
  worker = fun config =>
    f.own "worker/%{config.name}" (workload config),

  # ── oneshot ────────────────────────────────────────────────────────────
  # A one-time task (migration, backfill). Runs once and exits.
  #
  #   contrib.oneshot { name = "migrate", image = "...", env = { ... } }
  #
  oneshot = fun config =>
    workload (config & { oneshot = true, restart = 'no }),
}
