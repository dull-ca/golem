# examples/maintenance/config.ncl
#
# A "site down for maintenance" page, built from the lowest-level
# primitives (g.file, g.package, g.unit). No g.contrib. No g.container.
# Demonstrates:
#
#   - Building a parameterized template function from primitives
#   - Templating file content with `m%"..."%` interpolation
#   - Composing primitives with g.combine
#   - Tagging the whole thing with g.own so unapply is clean
#   - The `id` parameter — a stable name that lets multiple maintenance
#     pages coexist and lets you revert exactly the one you applied

let g = import "../../nickel/lib.ncl" in

# ── site_down_page — a reusable template ─────────────────────────────
#
# Returns a Fragment (resources + handlers) that puts a static "down
# for maintenance" page behind nginx for one host.
#
# Parameters:
#   host    — the domain to serve (e.g. "lichess.example.com")
#   date    — when you expect to be back
#   reason  — what you're doing
#   id      — a stable identifier for this incident. Used in file paths
#             and ownership so multiple maintenance pages don't collide
#             and you can roll back exactly the one you put up.
#
let site_down_page = fun config =>
  let html_path = "/var/www/maintenance/%{config.id}/index.html" in
  let site_path = "/etc/nginx/sites-enabled/%{config.host}.conf" in

  let html_content = m%"
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>%{config.host} — down for maintenance</title>
  <style>
    body { font-family: system-ui, sans-serif; max-width: 600px;
           margin: 4rem auto; padding: 2rem; text-align: center; }
    h1 { color: #333; }
    p  { color: #666; line-height: 1.6; }
    .badge { display: inline-block; padding: 0.25rem 0.75rem;
             background: #fef3c7; color: #92400e; border-radius: 4px;
             font-size: 0.875rem; }
  </style>
</head>
<body>
  <span class="badge">Maintenance — %{config.id}</span>
  <h1>We'll be back soon</h1>
  <p><strong>%{config.host}</strong> is temporarily unavailable.</p>
  <p>%{config.reason}</p>
  <p>Expected back: <strong>%{config.date}</strong></p>
</body>
</html>
"% in

  let site_content = m%"
# Managed by golem. Do not edit.
server {
    listen 443 ssl http2;
    listen [::]:443 ssl http2;
    server_name %{config.host};

    ssl_certificate     /etc/letsencrypt/live/%{config.host}/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/%{config.host}/privkey.pem;

    root  /var/www/maintenance/%{config.id};
    index index.html;

    # 503 the world, but serve our static page as the body.
    location / {
        return 503;
    }
    error_page 503 /index.html;
    location = /index.html {
        internal;
    }
}

# Plain HTTP -> redirect to HTTPS so curl-from-the-internet sees TLS.
server {
    listen 80;
    listen [::]:80;
    server_name %{config.host};
    return 301 https://%{config.host}$request_uri;
}
"% in

  g.own "site-down/%{config.id}" (
    g.combine [
      g.package { name = "nginx" },
      g.unit    { name = "nginx.service" },
      g.file    { path = html_path, content = html_content },
      g.file    { path = site_path, content = site_content },
    ]
  )
in

# ── inventory ─────────────────────────────────────────────────────────
let inventory = {
  hosts = {
    "edge-01" = {
      public_ip = "203.0.113.10",
      vrack_ip  = "10.42.0.1",

      fragments = [
        site_down_page {
          host   = "lichess.example.com",
          date   = "May 22nd, 2026",
          reason = "Cleaning out the cobwebs",
          id     = "temp-litour-down-2022",
        },
      ],
    },
  },
} in

{ bundle_for = g.bundle_for inventory }
