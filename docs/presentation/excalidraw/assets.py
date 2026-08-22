"""Committed image files, turned into the data URLs Excalidraw embeds.

The build is offline: nothing here fetches. A file lands in `assets/` by hand,
with its licence and provenance recorded in `assets/README.md`, and this module
only reads it.

`file_id` is a blake2s digest of the bytes rather than a counter, so the same
artwork keeps the same id across builds and across the scenes that use it —
`Scene.image` writes one `files` entry per distinct file, however many elements
point at it.
"""

from __future__ import annotations

import base64
import hashlib
import re
from functools import lru_cache
from pathlib import Path
from typing import NamedTuple

ASSET_DIRECTORY = Path(__file__).resolve().parent.parent / "assets"

SVG_MIME_TYPE = "image/svg+xml"
VIEW_BOX = re.compile(
    r'viewBox\s*=\s*"\s*[-\d.]+\s+[-\d.]+\s+([\d.]+)\s+([\d.]+)\s*"'
)


class EmbeddedImage(NamedTuple):
    file_id: str
    mime_type: str
    data_url: str
    aspect: float


# NOTE: an SVG data URL carries no intrinsic pixel size, so a caller giving only a
# height needs the viewBox ratio to work out the width. An SVG without one cannot be
# placed, which is why this raises rather than assuming a square.
def _aspect_of(markup: str) -> float:
    box = VIEW_BOX.search(markup)
    if box is None:
        raise ValueError("an embedded SVG needs a viewBox to be measured by")
    width, height = float(box.group(1)), float(box.group(2))
    if width <= 0 or height <= 0:
        raise ValueError(f"an embedded SVG needs a positive viewBox, got {width}x{height}")
    return width / height


@lru_cache(maxsize=None)
def vendored_svg(filename: str) -> EmbeddedImage:
    payload = (ASSET_DIRECTORY / filename).read_bytes()
    return EmbeddedImage(
        file_id=hashlib.blake2s(payload, digest_size=20).hexdigest(),
        mime_type=SVG_MIME_TYPE,
        data_url=(
            f"data:{SVG_MIME_TYPE};base64,"
            + base64.b64encode(payload).decode("ascii")
        ),
        aspect=_aspect_of(payload.decode("utf-8")),
    )
