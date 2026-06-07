# blueprint.ncl — author Blueprints for golem.
#
#   let b = import "blueprint.ncl" in
#   { name = "web", hosts = [ ... ] } | b.Blueprint
{
  Workload = {
    name | String,
    image | String | default = "",
  },

  Service = {
    name | String,
    image | String | default = "",
    port | Number | default = 0,
  },

  Ingress = {
    name | String,
    service | String | default = "",
    from | String | default = "world", # "world" or "internal"
    port | Number | default = 0,
  },

  Host = {
    name | String,
    workloads | Array Workload | default = [],
    services | Array Service | default = [],
    ingress | Array Ingress | default = [],
  },

  Blueprint = {
    name | String,
    hosts | Array Host | default = [],
  },
}
