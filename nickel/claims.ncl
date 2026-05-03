# claims.ncl — low-level Claim contracts.
#
# Users don't write these directly. Higher-level contracts (Workload, Ingress)
# expand into these. The agent consumes exactly this shape.
{
  # ─── ClaimId ────────────────────────────────────────────────────────────
  ClaimId = {
    kind | [| 'File, 'AptPackage, 'SystemdUnit, 'Quadlet, 'NftFragment, 'CaddySite |],
    key  | String,
  },

  # ─── Per-resource specs ─────────────────────────────────────────────────
  FileSpec = {
    path    | String,
    content | String,                    # inline bytes; for large payloads
                                         # use content_ref (hash) — future
    mode    | Number | default = 420,    # 0o644
    owner   | String | default = "root",
    group   | String | default = "root",
    marker  | [| 'Owned, 'BlockInFile, 'Dropin |] | default = 'Owned,
  },

  AptPackageSpec = {
    name    | String,
    version | String | optional,
    hold    | Bool   | default = false,
  },

  SystemdUnitSpec = {
    name   | String,
    enable | Bool | default = true,
    active | Bool | default = true,
    scope  | [| 'System, 'User |] | default = 'System,
  },

  # Handler: "if source claim's content changes, after it applies, also
  # restart the listed units." Mirrors Ansible handlers.
  Handler = {
    source | ClaimId,
    targets | Array String,   # unit names: ["caddy.service"]
  },

  # ─── A Claim ────────────────────────────────────────────────────────────
  Claim = {
    id     | ClaimId,
    spec   | [| 'File FileSpec,
                'AptPackage AptPackageSpec,
                'SystemdUnit SystemdUnitSpec,
                'Quadlet { name | String, body | String },   # raw .container body
             |],
    owners | Array String,               # refcount keys, e.g. "workload/api"
    after  | Array ClaimId | default = [], # ordering deps
  },

  # ─── Bundle ─────────────────────────────────────────────────────────────
  # What the agent receives. One of these per node, signed by golemctl.
  Bundle = {
    version  | Number,          # monotonic, supplied by golemctl
    node     | String,          # this node's name
    claims   | Array Claim,
    handlers | Array Handler | default = [],
  },
}
