"""Fleet defaults: host names, the base image, and the port scheme.

Every ephemeral file the harness writes lives under `.fleet/` at the repo root,
so nothing escapes the checkout and `reset` can wipe it wholesale.
"""

from __future__ import annotations

import os
from dataclasses import dataclass, field
from pathlib import Path


# The default host set: the six lichess server names. `up` boots these unless
# --hosts or --count narrows it.
LICHESS_HOSTS = ["scaly", "manta", "orbit", "talos", "kaiju", "zulip"]

BASE_IMAGE_INDEX_URL = "https://cloud.debian.org/images/cloud/trixie/latest/"
BASE_IMAGE_PATTERN = "debian-13-genericcloud-amd64"
BASE_IMAGE_SUFFIX = ".qcow2"

GUEST_USER = "golem"
GUEST_MEMORY_MB = 2048
GUEST_CPUS = 2

# Each VM claims one slot `i` (its index in the boot list) and takes the same
# offset on every base: ssh on 2200+i, golemd on 8800+i (forwarded to the
# guest's 7474). Slot 0 → ssh 2200, golemd 8800; slot 1 → 2201/8801; and so on.
SSH_PORT_BASE = 2200
GOLEMD_PORT_BASE = 8800
GOLEMD_GUEST_PORT = 7474

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
    keypair, the state file, and one `vm-<name>/` per booted guest."""

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
    def ssh_key(self) -> Path:
        return self.fleet_dir / "id_ed25519"

    @property
    def ssh_pubkey(self) -> Path:
        return self.fleet_dir / "id_ed25519.pub"

    def vm_dir(self, name: str) -> Path:
        return self.fleet_dir / f"vm-{name}"


def paths() -> Paths:
    return Paths(root=repo_root())


@dataclass(frozen=True)
class HostPlan:
    """A host's name paired with the two forwarded ports its slot earns."""

    name: str
    ssh_port: int
    golemd_port: int


def plan_hosts(names: list[str]) -> list[HostPlan]:
    """Assign each name its slot's ports by list position — see the port scheme
    on SSH_PORT_BASE. Order fixes the ports, so the same name list always lands
    on the same ports."""
    plans: list[HostPlan] = []
    for index, name in enumerate(names):
        plans.append(
            HostPlan(
                name=name,
                ssh_port=SSH_PORT_BASE + index,
                golemd_port=GOLEMD_PORT_BASE + index,
            )
        )
    return plans
