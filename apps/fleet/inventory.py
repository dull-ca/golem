"""Render the booted VMs as the TOML inventory `golemctl fleet` reads (ADR 0038).

Each guest's golemd is forwarded to a loopback port on this machine, so the
inventory is just the state file's records as `name = url`. A VM's name is also
its scroll's name, which is what makes the file drivable: golemctl matches
inventory names against the manifest's scroll names and skips any host the
manifest is silent about.

The TOML is written by hand — the stdlib reads TOML but does not write it, and
this file is one flat table of strings.
"""

from __future__ import annotations

from typing import Iterable

from .state import VmRecord

_BARE_KEY_CHARS = frozenset(
    "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_-"
)


def golemd_url(golemd_port: int) -> str:
    """Where this machine reaches a guest's golemd: QEMU forwards the guest's
    `7474` to `golemd_port` on loopback."""
    return f"http://127.0.0.1:{golemd_port}"


def inventory_entries(records: Iterable[VmRecord]) -> list[tuple[str, str]]:
    """Each record as its `(name, url)` inventory entry, in the given order."""
    return [(record.name, golemd_url(record.golemd_port)) for record in records]


def _is_bare_toml_key(name: str) -> bool:
    return bool(name) and all(ch in _BARE_KEY_CHARS for ch in name)


def _toml_string(value: str) -> str:
    escaped = value.replace("\\", "\\\\").replace('"', '\\"')
    return f'"{escaped}"'


def _toml_key(name: str) -> str:
    return name if _is_bare_toml_key(name) else _toml_string(name)


def render_hosts_toml(entries: Iterable[tuple[str, str]]) -> str:
    """The `[hosts]` table, one line per entry in the order given. A name that
    is not a bare TOML key is quoted; both keys and urls are escaped, so a name
    or url carrying a quote or backslash still parses back."""
    lines = ["[hosts]"]
    lines.extend(f"{_toml_key(name)} = {_toml_string(url)}" for name, url in entries)
    return "\n".join(lines) + "\n"
