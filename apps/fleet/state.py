"""The fleet's memory of which VMs exist: `.fleet/state.json`, keyed by name.

One record per booted guest — its ports, its qemu pid, and where its disk,
pidfile, and console log live — so later commands can find and reach a VM
without re-deriving anything. Written eagerly on every mutation.
"""

from __future__ import annotations

import json
from dataclasses import dataclass, asdict
from pathlib import Path


@dataclass
class VmRecord:
    """One booted guest: its two forwarded ports, the qemu pid, and the paths
    to its overlay disk, pidfile, and serial-console log."""

    name: str
    ssh_port: int
    golemd_port: int
    pid: int
    disk: str
    pidfile: str
    console_log: str


class FleetState:
    """The `state.json` file loaded into a name → record map, saved on write."""

    def __init__(self, path: Path) -> None:
        self._path = path
        self._vms: dict[str, VmRecord] = {}
        self._load()

    def _load(self) -> None:
        if not self._path.exists():
            return
        raw = json.loads(self._path.read_text())
        for entry in raw.get("vms", []):
            record = VmRecord(**entry)
            self._vms[record.name] = record

    def _save(self) -> None:
        self._path.parent.mkdir(parents=True, exist_ok=True)
        payload = {"vms": [asdict(record) for record in self._vms.values()]}
        self._path.write_text(json.dumps(payload, indent=2) + "\n")

    def all(self) -> list[VmRecord]:
        return list(self._vms.values())

    def get(self, name: str) -> VmRecord | None:
        return self._vms.get(name)

    def put(self, record: VmRecord) -> None:
        self._vms[record.name] = record
        self._save()

    def remove(self, name: str) -> None:
        if name in self._vms:
            del self._vms[name]
            self._save()

    def clear(self) -> None:
        self._vms = {}
        self._save()
