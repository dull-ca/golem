# workload.ncl — the Workload contract.
#
# A Workload is a container image that should run on some set of nodes.
# It expands (at Fleet level) into: ensure podman installed, write a .container
# quadlet file, start the generated systemd unit.

let c = import "claims.ncl" in

{
  PortMapping = {
    host      | Number,
    container | Number,
    protocol  | [| 'tcp, 'udp |] | default = 'tcp,
  },

  VolumeMount = {
    source      | String,                      # host path or named volume
    destination | String,                      # container path
    read_only   | Bool | default = false,
  },

  # A Workload declared by the user.
  Workload = {
    name    | String,
    image   | String,
    nodes   | Array String,                    # which node names run this
    command | String | optional,
    user    | String | optional,
    ports   | Array PortMapping | default = [],
    volumes | Array VolumeMount | default = [],
    env     | { _ : String }    | default = {},
    # Restart policy maps to systemd Restart= on the generated unit.
    restart | [| 'always, 'on-failure, 'no' |] | default = 'always',
    # Tag for the image pull policy. If `digest` is set we pin by @sha256:.
    digest  | String | optional,

    # ── derived: the quadlet .container file body ───────────────────────────
    # Users can override this if they need escape hatches; by default we
    # construct it from the fields above.
    quadlet_body | String = render_quadlet name image command user ports volumes env restart digest,
  },

  # Render a Podman Quadlet .container file from a Workload.
  # Keep this aligned with `man quadlet` on trixie (podman 5.4).
  render_quadlet = fun name image command user ports volumes env restart digest =>
    let image_ref = if digest == null then image else "%{image}@%{digest}" in
    let header = m%"
      # Managed by golem. Do not edit.
      [Unit]
      Description=Golem workload: %{name}

      [Container]
      Image=%{image_ref}
      ContainerName=%{name}
    "% in
    let user_line =
      if user == null then "" else "User=%{user}\n" in
    let cmd_line =
      if command == null then "" else "Exec=%{command}\n" in
    let port_lines =
      ports
      |> std.array.map (fun p => "PublishPort=%{std.to_string p.host}:%{std.to_string p.container}/%{std.string.from_enum p.protocol}")
      |> std.string.join "\n" in
    let vol_lines =
      volumes
      |> std.array.map (fun v =>
          let ro = if v.read_only then ":ro" else "" in
          "Volume=%{v.source}:%{v.destination}%{ro}")
      |> std.string.join "\n" in
    let env_lines =
      env
      |> std.record.to_array
      |> std.array.map (fun p => "Environment=%{p.field}=%{p.value}")
      |> std.string.join "\n" in
    let install = m%"

      [Service]
      Restart=%{std.string.from_enum restart}

      [Install]
      WantedBy=multi-user.target default.target
    "% in
    header ++ user_line ++ cmd_line ++ port_lines ++ "\n" ++ vol_lines ++ "\n" ++ env_lines ++ install,

  # Expand a Workload into the claims it produces *for a given target node*.
  # Returns [] if this workload doesn't run on that node.
  workload_claims_for = fun w node =>
    if !(std.array.elem node w.nodes) then [] else
    let owner = "workload/%{w.name}" in
    let quadlet_path = "/etc/containers/systemd/%{w.name}.container" in
    let unit_name = "%{w.name}.service" in
    let file_id = { kind = 'File, key = quadlet_path } in
    [
      {
        id = { kind = 'AptPackage, key = "podman" },
        spec = 'AptPackage { name = "podman" },
        owners = [owner],
        after = [],
      },
      {
        id = file_id,
        spec = 'File {
          path = quadlet_path,
          content = w.quadlet_body,
          mode = 420,   # 0o644
          marker = 'Owned,
        },
        owners = [owner],
        after = [{ kind = 'AptPackage, key = "podman" }],
      },
      {
        id = { kind = 'SystemdUnit, key = unit_name },
        spec = 'SystemdUnit { name = unit_name, active = true, enable = true },
        owners = [owner],
        # Unit ordering: file must land first. daemon-reload is implicit in
        # the agent when any Quadlet-owned file changes.
        after = [file_id],
      },
    ],
}
