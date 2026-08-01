"""Render the booted VMs as the TOML inventory `golemctl fleet` reads (ADR 0038).

Each guest is an ssh-form host (ADR 0042): its `[hosts.<name>]` table carries
the ssh destination and forwarded ssh port, the fleet key and the host-checking
options every harness ssh uses, and the path to the shared token. There is no
url to write — the guests' daemons are loopback-bound, and golemctl opens its
own forward from these fields.

A VM's name is also its scroll's name, which is what makes the file drivable:
golemctl matches inventory names against the manifest's scroll names and skips
any host the manifest is silent about.

The TOML is written by hand — the stdlib reads TOML but does not write it.
"""

from __future__ import annotations

from typing import Iterable

from . import config
from .config import Paths
from .state import VmRecord

_BARE_KEY_CHARS = frozenset(
    "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_-"
)

SSH_ARGS = (
    "-o",
    "StrictHostKeyChecking=no",
    "-o",
    "UserKnownHostsFile=/dev/null",
    "-o",
    "LogLevel=ERROR",
)


class HostEntry:
    """One guest as golemctl needs to see it. `remote_port` is `None` for the
    usual case — the daemon on `config.GOLEMD_GUEST_PORT` — and is written out
    only when it differs, so the common inventory says nothing golemctl already
    assumes."""

    def __init__(
        self,
        name: str,
        ssh: str,
        ssh_port: int,
        ssh_args: list[str],
        token_file: str,
        remote_port: int | None = None,
    ) -> None:
        self.name = name
        self.ssh = ssh
        self.ssh_port = ssh_port
        self.ssh_args = ssh_args
        self.token_file = token_file
        self.remote_port = remote_port


def host_entry(paths: Paths, record: VmRecord) -> HostEntry:
    """A record as its inventory host. Paths are resolved absolute: golemctl may
    be run from anywhere, and a relative key or token path would be read against
    its working directory, not the repo root."""
    return HostEntry(
        name=record.name,
        ssh=f"{config.GUEST_USER}@127.0.0.1",
        ssh_port=record.ssh_port,
        ssh_args=["-i", str(paths.ssh_key.resolve()), *SSH_ARGS],
        token_file=str(paths.token_file.resolve()),
    )


def inventory_entries(paths: Paths, records: Iterable[VmRecord]) -> list[HostEntry]:
    """Each record as its inventory host, in the given order."""
    return [host_entry(paths, record) for record in records]


def _is_bare_toml_key(name: str) -> bool:
    return bool(name) and all(ch in _BARE_KEY_CHARS for ch in name)


def _toml_string(value: str) -> str:
    escaped = value.replace("\\", "\\\\").replace('"', '\\"')
    return f'"{escaped}"'


def _toml_key(name: str) -> str:
    return name if _is_bare_toml_key(name) else _toml_string(name)


def _toml_array(values: Iterable[str]) -> str:
    return "[" + ", ".join(_toml_string(value) for value in values) + "]"


def render_hosts_toml(entries: Iterable[HostEntry]) -> str:
    """One `[hosts.<name>]` block per entry, in the order given. A name that is
    not a bare TOML key is quoted, and every string is escaped, so a name, path,
    or ssh argument carrying a quote or backslash still parses back. No entries
    renders empty rather than a bare `[hosts]`: golemctl errors on an inventory
    with no hosts, which is the right answer for a fleet with none."""
    blocks: list[str] = []
    for entry in entries:
        lines = [f"[hosts.{_toml_key(entry.name)}]"]
        lines.append(f"ssh = {_toml_string(entry.ssh)}")
        lines.append(f"ssh_port = {entry.ssh_port}")
        if entry.remote_port is not None and entry.remote_port != config.GOLEMD_GUEST_PORT:
            lines.append(f"remote_port = {entry.remote_port}")
        lines.append(f"ssh_args = {_toml_array(entry.ssh_args)}")
        lines.append(f"token_file = {_toml_string(entry.token_file)}")
        blocks.append("\n".join(lines))
    if not blocks:
        return ""
    return "\n\n".join(blocks) + "\n"
