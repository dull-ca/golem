"""The two read-only views of a guest's golemd the harness shows.

Both go through an ssh forward (`tunnel`) bearing the fleet token, and both
return `None` on any failure — an unbooted guest, a daemon not up yet, a refused
token — because callers poll and tabulate rather than diagnose. `fleet status`
prints a dash; `deploy` retries until an answer arrives or the attempts run out.
An operator chasing *why* gets it from `journalctl -u golemd` on the guest.
"""

from __future__ import annotations

from typing import Any

import httpx

from . import tunnel
from .config import Paths
from .state import VmRecord


def status(paths: Paths, record: VmRecord, token: str, timeout: float = 5.0) -> dict[str, Any] | None:
    """The daemon's `/status` summary, or `None` if it does not answer."""
    try:
        return tunnel.get_json(paths, record, "status", token, request_timeout=timeout)
    except (httpx.HTTPError, ValueError, tunnel.TunnelError):
        return None


def state(paths: Paths, record: VmRecord, token: str, timeout: float = 5.0) -> dict[str, Any] | None:
    """The daemon's resolved `/state` view, or `None` if it does not answer."""
    try:
        return tunnel.get_json(paths, record, "state", token, request_timeout=timeout)
    except (httpx.HTTPError, ValueError, tunnel.TunnelError):
        return None
