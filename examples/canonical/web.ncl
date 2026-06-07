let b = import "../../nickel/blueprint.ncl" in
{
  name = "web",
  hosts = [
    {
      name = "web-01",
      workloads = [
        { name = "migrate", image = "myapp:latest" },
      ],
      services = [
        { name = "app", image = "myapp:latest", port = 8080 },
        { name = "cache", image = "redis:7", port = 6379 },
      ],
      ingress = [
        { name = "public", service = "app", from = "world", port = 443 },
      ],
    },
  ],
} | b.Blueprint
