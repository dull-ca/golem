# lib.ncl — the public surface for users.
#
# A user's config.ncl imports this and writes a Deploy. `golemctl` calls
# `bundle_for` with a node name to get the layer-3 bundle that ships to that
# node.
#
# This file is intentionally thin. The schemas live in app.ncl and deploy.ncl;
# the layer-2 translation that turns App+Deploy into Bundle claims is in
# golemctl proper (Rust), not Nickel — Nickel just exports the input shape
# and the resolver placeholders, then golemctl scans + substitutes + renders
# quadlets.

let c = import "claims.ncl" in
let a = import "app.ncl" in
let d = import "deploy.ncl" in

{
  # Re-export so user configs only need to import lib.ncl.
  App     = a.App,
  Service = a.Service,
  Volume  = a.Volume,

  Deploy  = d.Deploy,
  Node    = d.Node,
  Expose  = d.Expose,

  # Placeholder helpers — these emit the literal placeholder strings that
  # golemctl substitutes at translation time. Use them in env values and
  # config files for cross-service references.
  #
  #   env = { PG_URL = "%{ref.service "postgres"}:5432" },
  #
  # Equivalent to writing the string `"{postgres.service}:5432"` directly,
  # but compiler-checked: misspell a service name and `golemctl` will reject
  # the deploy because the placeholder doesn't resolve.
  ref = {
    service = fun name => "{%{name}.service}",
    port    = fun name p => "{%{name}.service}:%{std.to_string p}",
  },
}
