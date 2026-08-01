"""Read a guest's golemd through an ssh forward the harness opens itself.

The guests' daemons bind `127.0.0.1:7474` (ADR 0042), so the `8800+slot` QEMU
hostfwd reaches nothing: the only route in is ssh, which is the same route
`golemctl` takes for real hosts. `get_json` opens a forward, makes exactly one
authorized request through it, and tears the forward down again — the harness's
read-only verbs (`status`, `deploy`'s readiness poll) are occasional, so a
per-call forward costs less than keeping one alive per guest.

The wait matters: ssh returns before the forward is listening, so the local port
is polled until it answers rather than dialed straight away.
"""

from __future__ import annotations

import socket
import subprocess
import time
from typing import Any

import httpx

from . import config
from .config import Paths
from .state import VmRecord

CONNECT_TIMEOUT_S = 10.0
CONNECT_POLL_INTERVAL_S = 0.1
REQUEST_TIMEOUT_S = 5.0


class TunnelError(RuntimeError):
    pass


def free_local_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def ssh_forward_argv(
    paths: Paths, record: VmRecord, local_port: int, remote_port: int
) -> list[str]:
    return [
        "ssh",
        "-N",
        "-o",
        "ExitOnForwardFailure=yes",
        "-L",
        f"127.0.0.1:{local_port}:127.0.0.1:{remote_port}",
        "-i",
        str(paths.ssh_key),
        "-p",
        str(record.ssh_port),
        "-o",
        "StrictHostKeyChecking=no",
        "-o",
        "UserKnownHostsFile=/dev/null",
        "-o",
        "LogLevel=ERROR",
        f"{config.GUEST_USER}@127.0.0.1",
    ]


def _local_port_answers(port: int) -> bool:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.settimeout(0.5)
        return sock.connect_ex(("127.0.0.1", port)) == 0


def wait_for_local_port(port: int, timeout: float) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if _local_port_answers(port):
            return True
        time.sleep(CONNECT_POLL_INTERVAL_S)
    return False


def get_json(
    paths: Paths,
    record: VmRecord,
    path: str,
    token: str,
    remote_port: int = config.GOLEMD_GUEST_PORT,
    connect_timeout: float = CONNECT_TIMEOUT_S,
    request_timeout: float = REQUEST_TIMEOUT_S,
) -> Any:
    """GET one path from a guest's golemd, bearing `token`, and decode the JSON.
    Raises `TunnelError` if the forward never comes up and `httpx` errors for
    anything the daemon answered — including the 401 a wrong token earns. The
    forward is killed in `finally`, so no ssh survives a failed call."""
    local_port = free_local_port()
    proc = subprocess.Popen(
        ssh_forward_argv(paths, record, local_port, remote_port),
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    try:
        if not wait_for_local_port(local_port, connect_timeout):
            raise TunnelError(
                f"{record.name}: ssh forward to 127.0.0.1:{local_port} did not "
                f"come up within {connect_timeout}s"
            )
        response = httpx.get(
            f"http://127.0.0.1:{local_port}/{path.lstrip('/')}",
            headers={"Authorization": f"Bearer {token}"},
            timeout=request_timeout,
        )
        response.raise_for_status()
        return response.json()
    finally:
        proc.terminate()
        proc.wait()
