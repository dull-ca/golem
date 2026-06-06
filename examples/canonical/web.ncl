let g = import "../../nickel/lib.ncl" in
{
  name     = "web",
  packages = ["nginx", "curl", "git"],
} | g.Blueprint
