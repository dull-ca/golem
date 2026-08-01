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
    return HostEntry(
        name=record.name,
        ssh=f"{config.GUEST_USER}@127.0.0.1",
        ssh_port=record.ssh_port,
        ssh_args=["-i", str(paths.ssh_key.resolve()), *SSH_ARGS],
        token_file=str(paths.token_file.resolve()),
    )


def inventory_entries(paths: Paths, records: Iterable[VmRecord]) -> list[HostEntry]:
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
