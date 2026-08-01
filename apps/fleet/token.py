"""The shared secret every guest's golemd requires and every caller presents.

One token serves the whole local fleet (ADR 0042): `ensure_token` generates it
on first use and returns the same value forever after, so `deploy` writes to the
guests exactly what `status`, the rendered inventory, and `golemctl` will later
send. Rotating it is deleting `.fleet/golem-token` and redeploying.

The file is created mode 0600 — it is the credential, not a config value.
"""

from __future__ import annotations

import os
import secrets

from .config import Paths
from .vm import FleetError

TOKEN_BYTES = 32
TOKEN_FILE_MODE = 0o600


def ensure_token(paths: Paths) -> str:
    """The fleet's token, generated on the first call and read back on every
    later one. Safe to call from any verb — it is how each of them arrives at
    the same secret without an ordering rule between them."""
    if paths.token_file.exists():
        existing = paths.token_file.read_text().strip()
        if not existing:
            raise FleetError(
                f"the fleet token file {paths.token_file} is empty — delete it to "
                f"have the next command generate a fresh token, then redeploy"
            )
        return existing
    paths.fleet_dir.mkdir(parents=True, exist_ok=True)
    token = secrets.token_urlsafe(TOKEN_BYTES)
    handle = os.open(
        paths.token_file,
        os.O_CREAT | os.O_EXCL | os.O_WRONLY,
        TOKEN_FILE_MODE,
    )
    with os.fdopen(handle, "w") as file:
        file.write(token + "\n")
    return token
