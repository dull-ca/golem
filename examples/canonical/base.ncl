let g = import "../../nickel/lib.ncl" in
{
  name     = "base",
  packages = ["git", "curl"],
} | g.Blueprint
