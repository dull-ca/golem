"""The real lichess fleet, read from the Ansible inventory and written out here.

Thirty hosts, and for each one the units it carries — services, ingress,
databases and workloads — split by the inventory's own `managed` field. An entry
with `managed: false` exists on the host and Ansible does not touch it, which is
the definition of hand-managed; the field defaults to true, so an entry without
it is Ansible's.

`unknown` marks a host with no tool-managed unit and at most one unit recorded
at all: nothing a tool knows, and next to nothing written down anywhere.

Source: `lichess-sysadmin/ansible/inventory/hosts.yaml`, merging each host's
entry under `all` with the same host's entry under every group that adds units
to it. Names only — no addresses, keys or tokens belong on a slide.
"""

from __future__ import annotations

from typing import NamedTuple


class Host(NamedTuple):
    name: str
    tool_units: int
    hand_units: int
    unknown: bool = False

    @property
    def units(self) -> int:
        return self.tool_units + self.hand_units


HOSTS: tuple[Host, ...] = (
    Host("achoo", 2, 0),
    Host("apate", 1, 3),
    Host("bookd", 0, 2),
    Host("bwrdd", 0, 2),
    Host("cobar", 5, 0),
    Host("dingo", 3, 0),
    Host("eight", 0, 2),
    Host("feck1", 0, 1, unknown=True),
    Host("feck2", 0, 1, unknown=True),
    Host("gappa", 0, 1, unknown=True),
    Host("image", 0, 2),
    Host("kaiju", 0, 1, unknown=True),
    Host("krakn", 0, 1, unknown=True),
    Host("lucid", 1, 0),
    Host("manta", 0, 8),
    Host("orbit", 8, 0),
    Host("pingu", 0, 1, unknown=True),
    Host("plato", 0, 2),
    Host("radio", 1, 3),
    Host("scaly", 0, 0, unknown=True),
    Host("sirch", 0, 3),
    Host("snafu", 0, 5),
    Host("sofia", 0, 1, unknown=True),
    Host("starr", 0, 2),
    Host("study", 0, 2),
    Host("syrup", 0, 1, unknown=True),
    Host("taffy", 0, 1, unknown=True),
    Host("talos", 0, 11),
    Host("thonk", 1, 0),
    Host("zulip", 0, 4),
)

HOST_COUNT = len(HOSTS)
UNIT_COUNT = sum(host.units for host in HOSTS)
TOOL_UNIT_COUNT = sum(host.tool_units for host in HOSTS)
HAND_UNIT_COUNT = sum(host.hand_units for host in HOSTS)
TOOL_KEPT_HOSTS = tuple(host.name for host in HOSTS if host.tool_units)
HAND_KEPT_HOST_COUNT = HOST_COUNT - len(TOOL_KEPT_HOSTS)
