# manta — the main lila application host.
#
# From inventory/hosts.py: Host "manta" runs the lila service (DSL name
# "lichess", port 9663), an internal redis-server (6379, used by orbit/starr),
# and fronts lichess.org / lichess1.org via nginx ingress from the world on 443.
# leroyjenkins-lila / leroyjenkins-nginx are local log-watching ban workloads
# (no port — they tail logs and ban, they don't serve traffic).
let b = import "../../nickel/blueprint.ncl" in
{
  name = "manta",
  hosts = [
    {
      name = "manta",
      workloads = [
        { name = "leroyjenkins-lila", image = "lichess-org/leroyjenkins:latest" },
        { name = "leroyjenkins-nginx", image = "lichess-org/leroyjenkins:latest" },
      ],
      services = [
        { name = "lila", image = "lichess-org/lila:latest", port = 9663 },
        { name = "redis-server", image = "redis:7", port = 6379 },
      ],
      ingress = [
        { name = "lichess.org", service = "lila", from = "world", port = 443 },
        { name = "lichess1.org", service = "lila", from = "world", port = 443 },
      ],
    },
  ],
} | b.Blueprint
