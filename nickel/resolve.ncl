# resolve.ncl — cross-host reference resolution.
#
# When users write `g.ref.service "mongo-lichess"` in env values, it
# produces a placeholder string like `{mongo-lichess.service}`. The
# resolver walks the inventory, finds which host owns that service and
# at what port, and substitutes the placeholder with `host:port`.
#
# Resolution happens at translation time (when `bundle_for` runs), not
# at agent time. Misspell a service name and the translation rejects
# before anything reaches an agent.

{
  # ── resolve_placeholders ───────────────────────────────────────────────
  # Walk a string and replace all `{name.service}` and `{name.host}`
  # placeholders with resolved addresses.
  #
  # inventory: the top-level config record with `hosts`
  # text: the string containing placeholders
  #
  resolve_string = fun inventory text =>
    let service_map = build_service_map inventory in
    let host_map    = build_host_map inventory in
    text
    |> resolve_service_refs service_map
    |> resolve_host_refs host_map,

  # ── resolve all env vars in a resource ─────────────────────────────────
  # Walks the spec of a resource and resolves placeholders in string values.
  resolve_resource = fun inventory res =>
    let resolve = resolve_string inventory in
    let new_spec =
      # For file claims, resolve placeholders in content
      if res.kind == "file" then
        res.spec
        |> std.record.remove "content"
        |> std.record.insert "content" (resolve res.spec.content)
      else res.spec
    in
    res
    |> std.record.remove "spec"
    |> std.record.insert "spec" new_spec,

  # ── resolve all resources in a fragment ────────────────────────────────
  resolve_fragment = fun inventory fragment => {
    resources = fragment.resources
      |> std.array.map (resolve_resource inventory),
    handlers = fragment.handlers,
  },

  # ── build_service_map ──────────────────────────────────────────────────
  # Walk all hosts, collect services into { service_name -> "host:port" }.
  build_service_map = fun inventory =>
    inventory.hosts
    |> std.record.to_array
    |> std.array.flat_map (fun host_entry =>
      let host = host_entry.value in
      let addr = host_address host in
      let services =
        if std.record.has_field "services" host
        then host.services
        else []
      in
      services
      |> std.array.map (fun svc =>
        { field = svc.name, value = "%{addr}:%{std.to_string svc.port}" }))
    |> std.record.from_array,

  # ── build_host_map ─────────────────────────────────────────────────────
  # Walk all hosts, collect into { host_name -> "ip_address" }.
  build_host_map = fun inventory =>
    inventory.hosts
    |> std.record.to_array
    |> std.array.map (fun host_entry =>
      let host = host_entry.value in
      { field = host.name, value = host_address host })
    |> std.record.from_array,

  # ── build_group_map ────────────────────────────────────────────────────
  # Build group membership: explicit groups + tag-derived groups.
  # Returns { group_name -> [ip_address, ...] }
  build_group_map = fun inventory =>
    let hosts = inventory.hosts |> std.record.to_array in
    # Tag-derived groups: each tag on a host creates group membership
    let tag_groups =
      hosts
      |> std.array.flat_map (fun host_entry =>
        let host = host_entry.value in
        let tags =
          if std.record.has_field "tags" host then host.tags else []
        in
        tags |> std.array.map (fun tag =>
          { group = tag, addr = host_address host }))
    in
    # Explicit groups
    let explicit_groups =
      let groups =
        if std.record.has_field "groups" inventory
        then inventory.groups
        else {}
      in
      groups
      |> std.record.to_array
      |> std.array.flat_map (fun g =>
        g.value |> std.array.map (fun host_name =>
          let host = std.record.get host_name inventory.hosts in
          { group = g.field, addr = host_address host }))
    in
    # Merge into { group_name -> [addr, ...] }
    let all_entries = tag_groups @ explicit_groups in
    all_entries
    |> std.array.fold_left (fun acc entry =>
      let existing =
        if std.record.has_field entry.group acc
        then std.record.get entry.group acc
        else []
      in
      std.record.insert entry.group (existing @ [entry.addr]) acc
    ) {},

  # ── host_address ───────────────────────────────────────────────────────
  # Primary address for a host: prefer vrack_ip (private), fall back to
  # public_ip.
  host_address = fun host =>
    if std.record.has_field "vrack_ip" host && host.vrack_ip != null
    then host.vrack_ip
    else if std.record.has_field "public_ip" host && host.public_ip != null
    then host.public_ip
    else std.fail_with "host %{host.name} has no address",

  # ── resolve_service_refs ───────────────────────────────────────────────
  # Replace `{name.service}` placeholders with host:port.
  resolve_service_refs = fun service_map text =>
    service_map
    |> std.record.to_array
    |> std.array.fold_left (fun acc entry =>
      std.string.replace "{%{entry.field}.service}" entry.value acc
    ) text,

  # ── resolve_host_refs ──────────────────────────────────────────────────
  # Replace `{name.host}` placeholders with IP address.
  resolve_host_refs = fun host_map text =>
    host_map
    |> std.record.to_array
    |> std.array.fold_left (fun acc entry =>
      std.string.replace "{%{entry.field}.host}" entry.value acc
    ) text,

  # ── resolve_allow_from ─────────────────────────────────────────────────
  # Expand allow_from entries (host names and group names) into a flat
  # list of IP addresses for firewall rules.
  resolve_allow_from = fun inventory allow_from =>
    let host_map = build_host_map inventory in
    let group_map = build_group_map inventory in
    allow_from
    |> std.array.flat_map (fun entry =>
      # If it's a group name, expand to all member addresses
      if std.record.has_field entry group_map then
        std.record.get entry group_map
      # If it's a host name, resolve to its address
      else if std.record.has_field entry host_map then
        [std.record.get entry host_map]
      # Otherwise, treat as a raw IP/CIDR
      else [entry])
    # Deduplicate
    |> std.array.fold_left (fun acc addr =>
      if std.array.elem addr acc then acc else acc @ [addr]
    ) [],
}
