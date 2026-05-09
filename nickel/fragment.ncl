# fragment.ncl — the composable building block.
#
# A Fragment is what every primitive returns. It bundles the resources
# the agent will manage and any handlers (restart-on-change triggers).
#
# Compose fragments with `combine`, tag them with `own`, then hand the
# result to `bundle` to get a signed-ready Bundle.

{
  # ── Fragment ───────────────────────────────────────────────────────────
  # Every primitive (file, package, container, …) returns one of these.
  # Higher-level abstractions return them too — they're just fragments
  # built from smaller fragments.
  Fragment = {
    resources | Array { .. } | default = [],
    handlers  | Array { .. } | default = [],
  },

  # ── combine ────────────────────────────────────────────────────────────
  # Merge an array of Fragments into one. Concatenates resources and
  # handlers — no deduplication, no reordering. The bundle step handles
  # dedup if needed.
  combine = fun fragments => {
    resources =
      fragments
      |> std.array.map (fun f => f.resources)
      |> std.array.flatten,
    handlers =
      fragments
      |> std.array.map (fun f => f.handlers)
      |> std.array.flatten,
  },

  # ── own ────────────────────────────────────────────────────────────────
  # Set ownership on every resource in a fragment. Ownership is how the
  # agent tracks who needs a resource — when the last owner drops, the
  # resource is cleaned up.
  #
  #   g.own "workload/lila" (g.combine [ ... ])
  #
  own = fun owner fragment => {
    resources =
      fragment.resources
      |> std.array.map (fun r =>
        r
        |> std.record.remove "owners"
        |> std.record.insert "owners" (r.owners @ [owner])),
    handlers = fragment.handlers,
  },

  # ── helpers for building resources ─────────────────────────────────────

  # Build a ClaimId. Uses snake_case strings matching the agent wire format.
  claim_id = fun ckind ckey => { kind = ckind, key = ckey },

  # Build a resource (claim) record in the wire format the agent expects.
  # The agent uses serde flatten, so `kind` and `spec` appear at the top
  # level alongside `id`, `owners`, `after`.
  resource = fun ckind ckey cspec =>
    {
      id     = { kind = ckind, key = ckey },
      kind   = ckind,
      spec   = cspec,
      owners = [],
      after  = [],
    },

  # Add an ordering dependency: "this resource must come after that one."
  after = fun dep res =>
    res
    |> std.record.remove "after"
    |> std.record.insert "after" (res.after @ [dep]),
}
