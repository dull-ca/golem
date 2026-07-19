# scaly — a fishnet compute node (24 cores).
#
# From inventory/hosts.py: Host "scaly" runs only fishnet=Fishnet(cores=24).
# Fishnet dials OUT to the lila-fishnet endpoint to pull analysis work; it
# accepts no inbound traffic. So: a single networkless workload, no ingress.
let b = import "../../nickel/blueprint.ncl" in
{
  name = "scaly",
  hosts = [
    {
      name = "scaly",
      workloads = [
        { name = "fishnet", image = "lichess-org/fishnet:latest" },
      ],
    },
  ],
} | b.Blueprint
