from __future__ import annotations

from typing import Any

import httpx

from . import tunnel
from .config import Paths
from .state import VmRecord


def status(paths: Paths, record: VmRecord, token: str, timeout: float = 5.0) -> dict[str, Any] | None:
    try:
        return tunnel.get_json(paths, record, "status", token, request_timeout=timeout)
    except (httpx.HTTPError, ValueError, tunnel.TunnelError):
        return None


def state(paths: Paths, record: VmRecord, token: str, timeout: float = 5.0) -> dict[str, Any] | None:
    try:
        return tunnel.get_json(paths, record, "state", token, request_timeout=timeout)
    except (httpx.HTTPError, ValueError, tunnel.TunnelError):
        return None
