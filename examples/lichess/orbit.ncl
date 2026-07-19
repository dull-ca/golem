# orbit — the lila supporting-services host.
#
# From inventory/hosts.py: Host "orbit" runs a cluster of internal lila
# microservices, plus two world ingresses fronted by nginx:
#   - lila-http     (9080)  -> http.lichess.org
#   - lila-engine   (9666)  -> engine.lichess.ovh
#   - lila-gif      (6175)  internal only (allow_from manta/starr/cobar)
#   - lila-fishnet  (9665)  internal only (the work dispatcher fishnet dials)
#   - lila-ip2proxy (1929)  internal only
#   - lila-push     (9054)  internal only
let b = import "../../nickel/blueprint.ncl" in
{
  name = "orbit",
  hosts = [
    {
      name = "orbit",
      services = [
        { name = "lila-http", image = "lichess-org/lila-http:latest", port = 9080 },
        { name = "lila-engine", image = "lichess-org/lila-engine:latest", port = 9666 },
        { name = "lila-gif", image = "lichess-org/lila-gif:latest", port = 6175 },
        { name = "lila-fishnet", image = "lichess-org/lila-fishnet:latest", port = 9665 },
        { name = "lila-ip2proxy", image = "lichess-org/lila-ip2proxy:latest", port = 1929 },
        { name = "lila-push", image = "lichess-org/lila-push:latest", port = 9054 },
      ],
      ingress = [
        { name = "http.lichess.org", service = "lila-http", from = "world", port = 443 },
        { name = "engine.lichess.ovh", service = "lila-engine", from = "world", port = 443 },
      ],
    },
  ],
} | b.Blueprint
