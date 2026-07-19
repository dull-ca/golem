# kaiju — lichess MongoDB primary.
#
# From inventory/hosts.py: Host "kaiju" runs mongo=[lichess_primary].
# MongoDB lives on the internal vrack only (172.16.0.50) and is reachable by the
# MONGODB_CLIENTS list — never the world. So: one internal service, no ingress.
let b = import "../../nickel/blueprint.ncl" in
{
  name = "kaiju",
  hosts = [
    {
      name = "kaiju",
      services = [
        { name = "mongod-lichess-primary", image = "mongo:7", port = 27017 },
      ],
    },
  ],
} | b.Blueprint
