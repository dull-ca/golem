"""HTTP client for a guest's golemd, reached over its forwarded port on the host.

`status` and `state` return `None` on any failure — the daemon may not be up
yet, so callers poll. `apply_manifest` returns the raw response so the caller
can distinguish HTTP status codes.
"""

from __future__ import annotations

from typing import Any

import httpx

from .state import VmRecord


def _base_url(record: VmRecord) -> str:
    # The guest's golemd is reachable on the host at its forwarded port.
    return f"http://127.0.0.1:{record.golemd_port}"


def status(record: VmRecord, timeout: float = 5.0) -> dict[str, Any] | None:
    """The daemon's `/status` summary, or `None` if it does not answer."""
    try:
        response = httpx.get(_base_url(record) + "/status", timeout=timeout)
        response.raise_for_status()
        return response.json()
    except (httpx.HTTPError, ValueError):
        return None


def state(record: VmRecord, timeout: float = 5.0) -> dict[str, Any] | None:
    """The daemon's resolved `/state` view, or `None` if it does not answer."""
    try:
        response = httpx.get(_base_url(record) + "/state", timeout=timeout)
        response.raise_for_status()
        return response.json()
    except (httpx.HTTPError, ValueError):
        return None


_APPLY_TIMEOUT = httpx.Timeout(connect=10.0, read=None, write=60.0, pool=10.0)


def apply_manifest(record: VmRecord, manifest: bytes, timeout: httpx.Timeout = _APPLY_TIMEOUT) -> httpx.Response:
    """POST a compiled manifest to `/manifest` as raw bytes. Returns the
    response unmapped so the caller reads the status code and revision body.
    A reconcile can run tens of minutes on a cold host (apt update, package
    installs, image pulls, canary retry rounds), so the read timeout is
    unbounded; only connect/write/pool are bounded."""
    return httpx.post(
        _base_url(record) + "/manifest",
        content=manifest,
        headers={"Content-Type": "application/octet-stream"},
        timeout=timeout,
    )
