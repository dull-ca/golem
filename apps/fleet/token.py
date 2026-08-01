"""The shared secret every guest's golemd requires and every caller presents.

One token serves the whole local fleet (ADR 0042): `ensure_token` generates it
on first use and returns the same value forever after, so `deploy` writes to the
guests exactly what `status`, the rendered inventory, and `golemctl` will later
send. Rotating it is deleting `.fleet/golem-token` and redeploying.

The file is created mode 0600 — it is the credential, not a config value.
"""

from __future__ import annotations

import secrets

from .config import Paths

TOKEN_BYTES = 32


def ensure_token(paths: Paths) -> str:
    """The fleet's token, generated on the first call and read back on every
    later one. Safe to call from any verb — it is how each of them arrives at
    the same secret without an ordering rule between them."""
    if paths.token_file.exists():
        return paths.token_file.read_text().strip()
    paths.fleet_dir.mkdir(parents=True, exist_ok=True)
    token = secrets.token_urlsafe(TOKEN_BYTES)
    paths.token_file.write_text(token + "\n")
    paths.token_file.chmod(0o600)
    return token
