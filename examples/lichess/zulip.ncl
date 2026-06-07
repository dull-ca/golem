# zulip — the team-chat host.
#
# From inventory/hosts.py: Host "zulip" runs the zulip-email service (SMTP, 25),
# a postgresql and a redis backing store, and the hq.lichess.ovh world ingress.
let b = import "../../nickel/blueprint.ncl" in
{
  name = "zulip",
  hosts = [
    {
      name = "zulip",
      services = [
        { name = "zulip", image = "zulip/docker-zulip:latest", port = 80 },
        { name = "zulip-email", image = "zulip/docker-zulip:latest", port = 25 },
        { name = "postgresql", image = "zulip/zulip-postgresql:14", port = 5432 },
        { name = "redis", image = "redis:7", port = 6379 },
      ],
      ingress = [
        { name = "hq.lichess.ovh", service = "zulip", from = "world", port = 443 },
      ],
    },
  ],
} | b.Blueprint
