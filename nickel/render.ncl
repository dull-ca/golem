# render.ncl — template renderers for system config files.
#
# Pure functions: config record → string. Each renderer produces the
# content for a file that a primitive will write to disk.

{
  # ── Quadlet .container file ────────────────────────────────────────────
  # Produces a Podman Quadlet .container file. Aligned with `man quadlet`
  # on Debian trixie (podman 5.x).
  #
  # Input: {
  #   name, image,
  #   command?, user?, env?, volume?, volumes?, configs?,
  #   digest?, restart?, oneshot?
  # }
  quadlet = fun config =>
    let image_ref =
      if std.record.has_field "digest" config && config.digest != null
      then "%{config.image}@%{config.digest}"
      else config.image
    in

    let env =
      if std.record.has_field "env" config then config.env else {}
    in
    let env_lines =
      env
      |> std.record.to_array
      |> std.array.map (fun p => "Environment=%{p.field}=%{p.value}")
      |> std.string.join "\n"
    in

    let user_line =
      if std.record.has_field "user" config && config.user != null
      then "User=%{config.user}\n"
      else ""
    in

    let cmd_line =
      if std.record.has_field "command" config && config.command != null
      then "Exec=%{config.command}\n"
      else ""
    in

    # Single volume: { at = "/path", size? = "10Gb" }
    let single_vol =
      if std.record.has_field "volume" config && config.volume != null then
        let vol = config.volume in
        let vol_name = "%{config.name}-data" in
        ["Volume=%{vol_name}.volume:%{vol.at}"]
      else []
    in

    # Named volumes: { name = { at = "/path" }, ... }
    let named_vols =
      if std.record.has_field "volumes" config then
        config.volumes
        |> std.record.to_array
        |> std.array.map (fun p =>
          let vol_name = "%{config.name}-%{p.field}" in
          "Volume=%{vol_name}.volume:%{p.value.at}")
      else []
    in

    let vol_lines =
      (single_vol @ named_vols)
      |> std.string.join "\n"
    in

    # Inline config files mounted into the container
    let config_lines =
      if std.record.has_field "configs" config then
        config.configs
        |> std.array.map (fun c => "Mount=type=bind,src=/etc/golem/configs/%{config.name}%{c.path},dst=%{c.path},ro=true")
        |> std.string.join "\n"
      else ""
    in

    let restart =
      if std.record.has_field "restart" config then
        std.string.from_enum config.restart
      else "always"
    in

    let service_type =
      if std.record.has_field "oneshot" config && config.oneshot then
        "Type=oneshot\nRemainAfterExit=no\n"
      else ""
    in

    m%"
# Managed by golem. Do not edit.
[Unit]
Description=Golem workload: %{config.name}

[Container]
Image=%{image_ref}
ContainerName=%{config.name}
%{user_line}%{cmd_line}%{vol_lines}
%{env_lines}
%{config_lines}

[Service]
%{service_type}Restart=%{restart}

[Install]
WantedBy=multi-user.target default.target
"%,

  # ── Quadlet .volume file ───────────────────────────────────────────────
  # Produces a Podman Quadlet .volume file for named volumes.
  volume = fun name =>
    m%"
# Managed by golem. Do not edit.
[Volume]
"%,

  # ── nftables fragment ──────────────────────────────────────────────────
  # Produces an nftables rule fragment. Dropped into /etc/nftables.d/ and
  # included by the main ruleset.
  #
  # Input: { name, port, protocol?, allow }
  #   allow: array of IP addresses/CIDRs
  nftables = fun config =>
    let protocol =
      if std.record.has_field "protocol" config
      then config.protocol
      else "tcp"
    in
    let port_str = std.to_string config.port in
    if std.array.length config.allow == 0 then
      # No sources allowed — deny all (default nftables behavior, emit
      # an explicit drop rule for documentation)
      m%"
# Managed by golem. Do not edit.
# %{config.name}: port %{port_str}/%{protocol} — no sources allowed
add rule inet filter input %{protocol} dport %{port_str} drop
"%
    else
      let sources =
        config.allow
        |> std.string.join ", "
      in
      m%"
# Managed by golem. Do not edit.
# %{config.name}: port %{port_str}/%{protocol}
add rule inet filter input %{protocol} dport %{port_str} ip saddr { %{sources} } accept
"%,

  # ── nginx site block ───────────────────────────────────────────────────
  # Produces an nginx server block for a site.
  #
  # Input: {
  #   host, upstream,
  #   tls?, rate_limit?, rate_limit_burst?, rate_limit_zone_size?,
  #   conn_limit?, conn_limit_zone_size?,
  #   cors?, buffering?, routes?, raw?,
  #   client_body_timeout?, client_header_timeout?, backlog?
  # }
  nginx = fun config =>
    let get = fun field fallback =>
      if std.record.has_field field config then
        std.record.get field config
      else fallback
    in
    let tls = get "tls" "auto" in
    let cors = get "cors" false in
    let buffering = get "buffering" true in
    let routes = get "routes" [] in
    let raw = get "raw" null in

    # Rate limiting zone (defined at http level, but we emit it in the
    # site block as a comment for reference — the actual zone goes in a
    # separate conf.d fragment)
    let rate_limit_directives =
      if std.record.has_field "rate_limit" config then
        let burst =
          if std.record.has_field "rate_limit_burst" config
          then "burst=%{std.to_string config.rate_limit_burst} nodelay"
          else ""
        in
        "    limit_req zone=%{std.string.replace "." "_" config.host} %{burst};\n"
      else ""
    in

    let conn_limit_directives =
      if std.record.has_field "conn_limit" config then
        "    limit_conn perip %{std.to_string config.conn_limit};\n"
      else ""
    in

    let buffering_directive =
      if buffering then "" else "    proxy_buffering off;\n"
    in

    let cors_directives =
      if cors then
        m%"
    add_header Access-Control-Allow-Origin $http_origin always;
    add_header Access-Control-Allow-Methods 'GET, POST, OPTIONS' always;
    add_header Access-Control-Allow-Headers 'DNT,User-Agent,X-Requested-With,If-Modified-Since,Cache-Control,Content-Type,Range,Authorization' always;
    add_header Access-Control-Allow-Credentials 'true' always;
"%
      else ""
    in

    let timeout_directives =
      let body_t =
        if std.record.has_field "client_body_timeout" config
        then "    client_body_timeout %{config.client_body_timeout};\n"
        else ""
      in
      let header_t =
        if std.record.has_field "client_header_timeout" config
        then "    client_header_timeout %{config.client_header_timeout};\n"
        else ""
      in
      "%{body_t}%{header_t}"
    in

    let proxy_headers = m%"
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
"% in

    # Route-specific location blocks
    let route_blocks =
      routes
      |> std.array.map (fun r =>
        m%"
    location %{r.path} {
        proxy_pass http://%{r.backend}_backend;
%{proxy_headers}
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
    }
"%)
      |> std.string.join "\n"
    in

    let raw_block =
      if raw != null then "\n    %{raw}\n" else ""
    in

    m%"
# Managed by golem. Do not edit.
upstream %{config.host |> std.string.replace "." "_"}_backend {
    server %{config.upstream};
}

server {
    listen 443 ssl http2;
    listen [::]:443 ssl http2;
    server_name %{config.host};

    ssl_certificate /etc/letsencrypt/live/%{config.host}/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/%{config.host}/privkey.pem;

%{rate_limit_directives}%{conn_limit_directives}%{buffering_directive}%{timeout_directives}%{cors_directives}
    location / {
        proxy_pass http://%{config.host |> std.string.replace "." "_"}_backend;
%{proxy_headers}    }
%{route_blocks}%{raw_block}
}
"%,

  # ── nginx rate limit zone ──────────────────────────────────────────────
  # Produces a rate limit zone config fragment for nginx http-level config.
  nginx_rate_limit_zone = fun config =>
    let zone_name = config.host |> std.string.replace "." "_" in
    let zone_size =
      if std.record.has_field "rate_limit_zone_size" config
      then config.rate_limit_zone_size
      else "10m"
    in
    "limit_req_zone $binary_remote_addr zone=%{zone_name}:%{zone_size} rate=%{config.rate_limit};",

  # ── nginx conn limit zone ──────────────────────────────────────────────
  nginx_conn_limit_zone = fun config =>
    let zone_size =
      if std.record.has_field "conn_limit_zone_size" config
      then config.conn_limit_zone_size
      else "10m"
    in
    "limit_conn_zone $binary_remote_addr zone=perip:%{zone_size};",
}
