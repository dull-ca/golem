from __future__ import annotations

import secrets

from .config import Paths

TOKEN_BYTES = 32


def ensure_token(paths: Paths) -> str:
    if paths.token_file.exists():
        return paths.token_file.read_text().strip()
    paths.fleet_dir.mkdir(parents=True, exist_ok=True)
    token = secrets.token_urlsafe(TOKEN_BYTES)
    paths.token_file.write_text(token + "\n")
    paths.token_file.chmod(0o600)
    return token
