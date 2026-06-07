# talos — the monitoring / infra-services host.
#
# From inventory/hosts.py: Host "talos" is the monitoring_hosts + loki_hosts
# node. It carries influxd (8086), alertmanager (9093), dns (53), ntpd (123),
# rsyslogd (514), a fishnet compute instance, and a stack of *.lichess.ovh
# ingresses (monitor / prom / alertmanager) fronted by nginx.
let b = import "../../nickel/blueprint.ncl" in
{
  name = "talos",
  hosts = [
    {
      name = "talos",
      workloads = [
        { name = "fishnet", image = "lichess-org/fishnet:latest" },
      ],
      services = [
        { name = "influxd", image = "influxdb:2", port = 8086 },
        { name = "alertmanager", image = "prom/alertmanager:latest", port = 9093 },
        { name = "prometheus", image = "prom/prometheus:latest", port = 9090 },
        { name = "loki", image = "grafana/loki:latest", port = 3100 },
        { name = "dns", image = "lichess-org/dnsmasq:latest", port = 53 },
        { name = "ntpd", image = "cturra/ntp:latest", port = 123 },
        { name = "rsyslogd", image = "rsyslog/rsyslog:latest", port = 514 },
      ],
      ingress = [
        { name = "monitor.lichess.ovh", service = "prometheus", from = "world", port = 443 },
        { name = "prom.lichess.ovh", service = "prometheus", from = "world", port = 443 },
        { name = "alertmanager.lichess.ovh", service = "alertmanager", from = "world", port = 443 },
      ],
    },
  ],
} | b.Blueprint
