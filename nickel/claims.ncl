# claims.ncl — low-level Claim contracts.
#
# Users don't write these directly. Higher-level contracts (Service,
# Workload, Ingress, Mongo, Database) expand into these. The agent
# consumes exactly this shape.
{
  # ─── ClaimId ────────────────────────────────────────────────────────────
  ClaimId = {
    kind | [|
      'File,
      'AptPackage,
      'SystemdUnit,
      'Quadlet,
      'NftFragment,
      'CaddySite,
      'NginxSite,
      'DnsRecord,
    |],
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

  # An nftables fragment: a named ruleset chunk dropped into
  # /etc/nftables.d/<key>.nft, included by the main ruleset, applied
  # atomically with `nft -f`. Used to materialize Service.allow_from
  # firewall rules.
  NftFragmentSpec = {
    name    | String,                                 # filename (without .nft)
    table   | [| 'Inet, 'Ip, 'Ip6 |] | default = 'Inet,
    body    | String,                                 # raw nft rule text
  },

  # A DNS record. Owned by whichever host carries the DNS role for the
  # zone (see Inventory.role_deps). When Ingress.dns = true the
  # translator emits one of these per ingress hostname pointing at the
  # ingress host's public addresses.
  DnsRecordSpec = {
    zone     | String,                                # "example.com"
    name     | String,                                # "@", "www", "lichess.org."
    kind     | [| 'A, 'AAAA, 'CNAME, 'TXT, 'MX |],
    value    | String,                                # ip/host/text — sigil per kind
    ttl      | Number | default = 300,
    priority | Number | optional,                    # MX priority
  },

  # Handler: "if source claim's content changes, after it applies, also
  # restart the listed units." Mirrors Ansible handlers.
  Handler = {
    source  | ClaimId,
    targets | Array String,                          # unit names: ["caddy.service"]
  },

  # ─── A Claim ────────────────────────────────────────────────────────────
  Claim = {
    id     | ClaimId,
    spec   | [|
      'File FileSpec,
      'AptPackage AptPackageSpec,
      'SystemdUnit SystemdUnitSpec,
      'Quadlet { name | String, body | String },     # raw .container body
      'NftFragment NftFragmentSpec,
      'DnsRecord DnsRecordSpec,
    |],
    owners | Array String,                           # refcount keys, e.g. "service/lila"
    after  | Array ClaimId | default = [],           # ordering deps
  },

  # ─── Bundle ─────────────────────────────────────────────────────────────
  # What the agent receives. One of these per host, signed by golemctl.
  Bundle = {
    version  | Number,                               # monotonic, supplied by golemctl
    node     | String,                               # this host's name
    claims   | Array Claim,
    handlers | Array Handler | default = [],
  },
}
