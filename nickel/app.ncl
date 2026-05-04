# app.ncl — the App contract.
#
# An App is a graph of named Services that wire to each other. It describes
# *what runs*, not *where* or *how the world reaches it*. Deployment lives in
# deploy.ncl.
#
# A Service is container-shaped (image, env, optional volume, optional config).
# Cross-service references use placeholder syntax: `{web.service}` resolves at
# translation time to the runtime address of the named service. The contract
# keeps placeholders as plain strings — the translator (golemctl) parses and
# substitutes them.

{
  # ── Volume ─────────────────────────────────────────────────────────────
  # A persistent named volume mounted into the service. `at` is the path
  # *inside* the container; `size` is sizing/quota info passed to the runtime.
  Volume = {
    at   | String,                                # container mount path
    size | String | optional,                     # "5Gb", "200Mb", etc.
  },

  # ── Service ────────────────────────────────────────────────────────────
  # One container in the app graph. Most fields are optional; a real-world
  # service often has just `image` plus an `env` block.
  Service = {
    image    | String,
    command  | String              | optional,    # override container CMD
    user     | String              | optional,
    env      | { _ : String }      | default = {},   # values may contain placeholders
    # Single-volume sugar: `volume = { at, size }`. For multiple, use `volumes`.
    volume   | Volume              | optional,
    volumes  | { _ : Volume }      | default = {},
    # An inline config file rendered into the container at a known path.
    # Used for things like a Caddyfile, a postgresql.conf override, an
    # nginx.conf — anything where the operator wants to ship config alongside
    # the service definition rather than baking it into the image.
    config   | { path | String, content | String } | optional,
    # Restart maps to systemd Restart= on the generated unit.
    restart  | [| 'always, 'on-failure, 'no' |] | default = 'always',
    # Pin by digest. If set, image is pulled as `image@sha256:…`.
    digest   | String              | optional,
    # `oneshot = true` runs the service as a Type=oneshot unit (used for
    # migrations, backfills, anything that should run once and exit).
    oneshot  | Bool                | default = false,
  },

  # ── App ────────────────────────────────────────────────────────────────
  # Just a record of services keyed by name. The keys are the service names
  # used in `{name.service}` placeholders.
  App = {
    services | { _ : Service },
  },
}
