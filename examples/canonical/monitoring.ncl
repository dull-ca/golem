let g = import "../../nickel/lib.ncl" in
{
  name     = "monitoring",
  packages = ["prometheus-node-exporter", "curl"],
} | g.Blueprint
