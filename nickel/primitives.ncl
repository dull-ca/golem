# primitives.ncl — the small building blocks.
#
# Each function takes a config record and returns a Fragment. Compose
# them with `combine` and `own` from fragment.ncl, then hand the result
# to `bundle` to get a signed-ready Bundle.
#
# These are the Ansible-module equivalents. Everything higher-level
# (workloads, services, postgres, redis) is built from these.

let f = import "fragment.ncl" in
let r = import "render.ncl" in

{
  # ── file ───────────────────────────────────────────────────────────────
  # Ensure a file exists with given content and permissions.
  #
  #   g.file { path = "/etc/foo.conf", content = "bar" }
  #   g.file { path = "/etc/foo.conf", content = "bar",
  #            mode = 384, owner = "app", group = "app" }
  #
  file = fun config => {
    resources = [
      f.resource "file" config.path {
        path    = config.path,
        content = config.content,
        mode    = if std.record.has_field "mode" config then config.mode else 420,
        owner   = if std.record.has_field "owner" config then config.owner else "root",
        group   = if std.record.has_field "group" config then config.group else "root",
        marker  = if std.record.has_field "marker" config then config.marker else "Owned",
      },
    ],
    handlers = [],
  },

  # ── package ────────────────────────────────────────────────────────────
  # Ensure an apt package is installed.
  #
  #   g.package { name = "nginx" }
  #   g.package { name = "postgres-18", version = "18.1-1", hold = true }
  #
  package = fun config => {
    resources = [
      f.resource "apt_package" config.name {
        name    = config.name,
        version =
          if std.record.has_field "version" config
          then config.version
          else null,
        hold    =
          if std.record.has_field "hold" config
          then config.hold
          else false,
      },
    ],
    handlers = [],
  },

  # ── unit ───────────────────────────────────────────────────────────────
  # Ensure a systemd unit is enabled and/or started.
  #
  #   g.unit { name = "nginx.service" }
  #   g.unit { name = "nginx.service", enable = true, active = true }
  #
  unit = fun config => {
    resources = [
      f.resource "systemd_unit" config.name {
        name   = config.name,
        enable = if std.record.has_field "enable" config then config.enable else true,
        active = if std.record.has_field "active" config then config.active else true,
        scope  = if std.record.has_field "scope" config then config.scope else "system",
      },
    ],
    handlers = [],
  },

  # ── container ──────────────────────────────────────────────────────────
  # Run a container via Podman Quadlet. This is a composed primitive:
  # it produces a .container file, any .volume files, a systemd unit,
  # and a handler to restart on config change.
  #
  #   g.container {
  #     name = "lila", image = "ghcr.io/lichess-org/lila:latest",
  #     env = { JAVA_OPTS = "-Xmx12g" },
  #     volume = { at = "/data", size = "100Gb" },
  #   }
  #
  container = fun config =>
    let quadlet_path = "/etc/containers/systemd/%{config.name}.container" in
    let unit_name    = "%{config.name}.service" in
    let quadlet_id   = f.claim_id "file" quadlet_path in
    let unit_id      = f.claim_id "systemd_unit" unit_name in

    let quadlet_content = r.quadlet config in

    # Volume files (Quadlet .volume)
    let volume_resources =
      let single =
        if std.record.has_field "volume" config && config.volume != null then
          let vol_name = "%{config.name}-data" in
          let vol_path = "/etc/containers/systemd/%{vol_name}.volume" in
          [{
            resource = f.resource "file" vol_path {
              path    = vol_path,
              content = r.volume vol_name,
              mode    = 420,
              owner   = "root",
              group   = "root",
              marker  = "Owned",
            },
            id = f.claim_id "file" vol_path,
          }]
        else []
      in
      let named =
        if std.record.has_field "volumes" config then
          config.volumes
          |> std.record.to_array
          |> std.array.map (fun p =>
            let vol_name = "%{config.name}-%{p.field}" in
            let vol_path = "/etc/containers/systemd/%{vol_name}.volume" in
            {
              resource = f.resource "file" vol_path {
                path    = vol_path,
                content = r.volume vol_name,
                mode    = 420,
                owner   = "root",
                group   = "root",
                marker  = "Owned",
              },
              id = f.claim_id "file" vol_path,
            })
        else []
      in
      single @ named
    in

    # Config files mounted into the container
    let config_resources =
      if std.record.has_field "configs" config then
        config.configs
        |> std.array.map (fun c =>
          let conf_path = "/etc/golem/configs/%{config.name}%{c.path}" in
          {
            resource = f.resource "file" conf_path {
              path    = conf_path,
              content = c.content,
              mode    = if std.record.has_field "mode" c then c.mode else 420,
              owner   = "root",
              group   = "root",
              marker  = "Owned",
            },
            id = f.claim_id "file" conf_path,
          })
      else []
    in

    let extra_file_ids =
      (volume_resources @ config_resources)
      |> std.array.map (fun x => x.id)
    in

    {
      resources =
        # Quadlet .container file, depends on volume/config files
        [
          f.resource "file" quadlet_path {
            path    = quadlet_path,
            content = quadlet_content,
            mode    = 420,
            owner   = "root",
            group   = "root",
            marker  = "Owned",
          }
          |> f.after (f.claim_id "apt_package" "podman"),

          # Systemd unit, depends on quadlet file
          f.resource "systemd_unit" unit_name {
            name   = unit_name,
            enable = true,
            active =
              if std.record.has_field "disabled" config && config.disabled
              then false
              else true,
            scope  = "system",
          }
          |> f.after quadlet_id,
        ]
        # Volume and config file resources
        @ (volume_resources |> std.array.map (fun x => x.resource))
        @ (config_resources |> std.array.map (fun x => x.resource)),

      handlers = [
        # Restart the service when the quadlet file changes
        { source = quadlet_id, targets = [unit_name] },
      ]
      # Also restart when config files change
      @ (config_resources |> std.array.map (fun x =>
        { source = x.id, targets = [unit_name] })),
    },

  # ── firewall ───────────────────────────────────────────────────────────
  # Drop an nftables rule fragment allowing traffic to a port from
  # specific sources.
  #
  #   g.firewall { name = "lila", port = 9663, allow = ["10.42.0.1"] }
  #   g.firewall { name = "lila", port = 9663, allow = ["10.42.0.1"],
  #                protocol = "udp" }
  #
  firewall = fun config =>
    let frag_name = "%{config.name}-allow" in
    let frag_path = "/etc/nftables.d/%{frag_name}.nft" in
    let frag_id = f.claim_id "file" frag_path in
    {
      resources = [
        f.resource "file" frag_path {
          path    = frag_path,
          content = r.nftables config,
          mode    = 420,
          owner   = "root",
          group   = "root",
          marker  = "Owned",
        },
      ],
      # Reload nftables when the fragment file changes
      handlers = [
        { source = frag_id, targets = ["nftables.service"] },
      ],
    },

  # ── dns_record ─────────────────────────────────────────────────────────
  # Manage a DNS record. (M3 — agent DNS provider is not yet implemented;
  # the primitive is here so configs can be written ahead of the agent.)
  #
  #   g.dns_record { zone = "example.com", name = "www",
  #                  type = "A", value = "203.0.113.10" }
  #
  dns_record = fun config => {
    resources = [
      f.resource "dns_record"
        "%{config.zone}/%{config.name}/%{config.type}" {
          zone     = config.zone,
          name     = config.name,
          kind     = config.type,
          value    = config.value,
          ttl      = if std.record.has_field "ttl" config then config.ttl else 300,
          priority =
            if std.record.has_field "priority" config
            then config.priority
            else null,
        },
    ],
    handlers = [],
  },

  # ── site ───────────────────────────────────────────────────────────────
  # Render an nginx site configuration file. Produces a file resource
  # and a handler to reload nginx on change.
  #
  #   g.site {
  #     host = "lichess.example.com",
  #     upstream = "10.42.0.2:9663",
  #   }
  #
  # Also accepts: tls, rate_limit, rate_limit_burst, rate_limit_zone_size,
  # conn_limit, conn_limit_zone_size, cors, buffering, routes, raw,
  # client_body_timeout, client_header_timeout, backlog.
  #
  site = fun config =>
    let site_path = "/etc/nginx/sites-enabled/%{config.host}.conf" in
    let site_id = f.claim_id "file" site_path in

    # Rate limit zone config (goes in conf.d, not the site block)
    let rate_zone_resources =
      if std.record.has_field "rate_limit" config then
        let zone_path = "/etc/nginx/conf.d/ratelimit-%{config.host}.conf" in
        [
          f.resource "file" zone_path {
            path    = zone_path,
            content = r.nginx_rate_limit_zone config,
            mode    = 420,
            owner   = "root",
            group   = "root",
            marker  = "Owned",
          },
        ]
      else []
    in

    # Conn limit zone config
    let conn_zone_resources =
      if std.record.has_field "conn_limit" config then
        let zone_path = "/etc/nginx/conf.d/connlimit-%{config.host}.conf" in
        [
          f.resource "file" zone_path {
            path    = zone_path,
            content = r.nginx_conn_limit_zone config,
            mode    = 420,
            owner   = "root",
            group   = "root",
            marker  = "Owned",
          },
        ]
      else []
    in

    {
      resources = [
        f.resource "file" site_path {
          path    = site_path,
          content = r.nginx config,
          mode    = 420,
          owner   = "root",
          group   = "root",
          marker  = "Owned",
        },
      ] @ rate_zone_resources @ conn_zone_resources,

      handlers = [
        { source = site_id, targets = ["nginx.service"] },
      ],
    },
}
