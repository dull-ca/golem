# manta-canary — a second blueprint that ALSO wants lila on manta.
#
# Not a real lichess host; it exists to demonstrate golem's refcounting. With
# both "manta" and "manta-canary" commissioned, the lila service on manta is
# wanted by two blueprints. Decommissioning "manta" therefore tears down manta's
# UNIQUE items (redis-server, the leroyjenkins workloads, the two ingresses) but
# LEAVES lila standing, because the canary still wants it.
let b = import "../../nickel/blueprint.ncl" in
{
  name = "manta-canary",
  hosts = [
    {
      name = "manta",
      services = [
        { name = "lila", image = "lichess-org/lila:latest", port = 9663 },
      ],
    },
  ],
} | b.Blueprint
