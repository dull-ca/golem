"""Fleet defaults: host names, the base image, and the port scheme.

Every ephemeral file the harness writes lives under `.fleet/` at the repo root,
so nothing escapes the checkout and `reset` can wipe it wholesale.
"""

from __future__ import annotations

import hashlib
import os
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional


# The default host set: the six lichess server names. `up` boots these unless
# --hosts or --count narrows it.
LICHESS_HOSTS = ["scaly", "manta", "orbit", "talos", "kaiju", "zulip"]

BASE_IMAGE_INDEX_URL = "https://cloud.debian.org/images/cloud/trixie/latest/"
BASE_IMAGE_PATTERN = "debian-13-genericcloud-amd64"
BASE_IMAGE_SUFFIX = ".qcow2"

GUEST_USER = "golem"
GUEST_MEMORY_MB = 2048
GUEST_CPUS = 2

SSH_PORT_BASE = 2200
GOLEMD_PORT_BASE = 8800
GOLEMD_GUEST_PORT = 7474
# A name's slot is `blake2b(name) mod PORT_SLOT_COUNT`, and its two forwarded
# ports are `SSH_PORT_BASE + slot` and `GOLEMD_PORT_BASE + slot`. The count
# bounds each range to 2200-2299 / 8800-8899 and caps the fleet at 100 distinct
# slots.
PORT_SLOT_COUNT = 100

SSH_READY_TIMEOUT_S = 180
SSH_POLL_INTERVAL_S = 3


def repo_root() -> Path:
    """The checkout root: `$DEVENV_ROOT` when set (the `fleet` script exports
    it), else the nearest ancestor holding `devenv.nix`. Relative `.emet` paths
    and `.fleet/` both anchor here."""
    env = os.environ.get("DEVENV_ROOT")
    if env:
        return Path(env)
    here = Path(__file__).resolve()
    for parent in here.parents:
        if (parent / "devenv.nix").exists():
            return parent
    return here.parents[2]


@dataclass(frozen=True)
class Paths:
    """Every path the harness reads or writes, derived from the repo `root`.
    All ephemeral state sits under `.fleet/`: the cached image, the fleet
    keypair, the state file, the rendered golemctl inventory, and one
    `vm-<name>/` per booted guest."""

    root: Path

    @property
    def fleet_dir(self) -> Path:
        return self.root / ".fleet"

    @property
    def images_dir(self) -> Path:
        return self.fleet_dir / "images"

    @property
    def state_file(self) -> Path:
        return self.fleet_dir / "state.json"

    @property
    def inventory_file(self) -> Path:
        return self.fleet_dir / "inventory.toml"

    @property
    def ssh_key(self) -> Path:
        return self.fleet_dir / "id_ed25519"

    @property
    def ssh_pubkey(self) -> Path:
        return self.fleet_dir / "id_ed25519.pub"

    def vm_dir(self, name: str) -> Path:
        return self.fleet_dir / f"vm-{name}"


def paths() -> Paths:
    return Paths(root=repo_root())


def slot_for_name(name: str) -> int:
    """The port slot a name owns: `blake2b(name) mod PORT_SLOT_COUNT`. Hashing
    the name (not its position in the boot list) means a name always maps to the
    same slot — and thus the same ssh/golemd ports — no matter what else is
    booted alongside it. The slots are collision-free across the fleet's known
    host names (the lichess set plus the registry/builder/puller/website dogfood
    boxes); a collision is only theoretically possible for arbitrary names
    outside that set."""
    digest = hashlib.blake2b(name.encode("utf-8"), digest_size=8).digest()
    return int.from_bytes(digest, "big") % PORT_SLOT_COUNT


@dataclass(frozen=True)
class HostPlan:
    """A host's name paired with the two forwarded ports its slot earns, plus
    any extra published host→guest tcp forwards (host_port, guest_port)."""

    name: str
    ssh_port: int
    golemd_port: int
    publish: tuple[tuple[int, int], ...] = ()


def _parse_port_pair(text: str) -> tuple[int, int]:
    if ":" in text:
        host_text, guest_text = text.split(":", 1)
    else:
        host_text = guest_text = text
    return int(host_text), int(guest_text)


def parse_publish(
    specs: Optional[list[str]], names: list[str]
) -> dict[str, tuple[tuple[int, int], ...]]:
    """Parse `--publish` specs into a host → forwards map. Each spec is either
    `NAME=HOST:GUEST` (published only on that host) or a bare `HOST:GUEST`
    (published on every booted host); a bare `PORT` means the same port on both
    sides. The forwards for each host preserve spec order."""
    result: dict[str, list[tuple[int, int]]] = {}
    for spec in specs or []:
        text = spec.strip()
        if not text:
            continue
        if "=" in text:
            target, port_spec = text.split("=", 1)
            targets = [target.strip()]
        else:
            port_spec = text
            targets = list(names)
        pair = _parse_port_pair(port_spec.strip())
        for target in targets:
            result.setdefault(target, []).append(pair)
    return {name: tuple(pairs) for name, pairs in result.items()}


def plan_hosts(
    names: list[str],
    publish: Optional[dict[str, tuple[tuple[int, int], ...]]] = None,
) -> list[HostPlan]:
    """Assign each name the ports its slot earns via `slot_for_name` — ssh on
    `SSH_PORT_BASE + slot`, golemd on `GOLEMD_PORT_BASE + slot`. The slot is
    derived from the name alone, so a name always lands on the same ports
    regardless of boot order or which other hosts are up. `publish` maps a host
    name to its extra forwards."""
    publish = publish or {}
    plans: list[HostPlan] = []
    for name in names:
        slot = slot_for_name(name)
        plans.append(
            HostPlan(
                name=name,
                ssh_port=SSH_PORT_BASE + slot,
                golemd_port=GOLEMD_PORT_BASE + slot,
                publish=publish.get(name, ()),
            )
        )
    return plans
