"""The two fleet-wide secrets: the bearer token that gates golemd, and the
AES-SIV key that seals values into manifests.

They are separate files with separate lifetimes on purpose. Rotating the token
locks out callers until every agent restarts; rotating the key additionally
invalidates every manifest already sealed with it. Folding them into one secret
would tie the cheaper rotation to the more expensive one.

The bearer token first.

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

SECRET_KEY_BYTES = 64
SECRET_KEY_HEX_CHARACTERS = SECRET_KEY_BYTES * 2


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


def ensure_secret_key(paths: Paths) -> str:
    """The fleet's AES-SIV key, generated on the first call and read back on
    every later one — the same shape as `ensure_token` and for the same reason:
    `emetc` seals a secret with it at compile time and every guest's golemd
    unseals with it at enact (ADR 0047), so both sides have to reach the same
    64 bytes without an ordering rule between them.

    Rotating it is deleting `.fleet/golem-secret-key` and redeploying — and then
    recompiling every manifest that carries a secret. A manifest sealed to the
    old key is undecodable by the new one, which is the point: golemd reports a
    key mismatch rather than enacting a stale credential.

    Mode 0600 from creation, via `O_EXCL` — this is the key, not a config value.
    """
    if paths.secret_key_file.exists():
        existing = paths.secret_key_file.read_text().strip()
        if not existing:
            raise FleetError(
                f"the fleet secret key file {paths.secret_key_file} is empty — delete "
                f"it to have the next command generate a fresh key, then redeploy "
                f"and recompile every manifest that carries a secret"
            )
        if not _is_fleet_key(existing):
            raise FleetError(
                f"the fleet secret key file {paths.secret_key_file} must hold "
                f"{SECRET_KEY_HEX_CHARACTERS} hexadecimal characters (a "
                f"{SECRET_KEY_BYTES}-byte AES-SIV key)"
            )
        return existing
    paths.fleet_dir.mkdir(parents=True, exist_ok=True)
    key = secrets.token_bytes(SECRET_KEY_BYTES).hex()
    handle = os.open(
        paths.secret_key_file,
        os.O_CREAT | os.O_EXCL | os.O_WRONLY,
        TOKEN_FILE_MODE,
    )
    with os.fdopen(handle, "w") as file:
        file.write(key + "\n")
    return key


def _is_fleet_key(text: str) -> bool:
    if len(text) != SECRET_KEY_HEX_CHARACTERS:
        return False
    try:
        bytes.fromhex(text)
    except ValueError:
        return False
    return True
