from __future__ import annotations

from excalidraw.layout import note, slide_header
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene

from . import one_host
from .glyph_ops import INSTALL

SLUG = "the-first-apply"
TITLE = "The first apply, and what golem wrote down"

SUBTITLE = (
    "Every glyph golem applies is written down with the inverse that undoes it."
)

REVISION = "revision 1"

CLOSING = "One apply, one revision: the ordered outcomes, and the inverse of each."


def build() -> Scene:
    scene = Scene(SLUG)
    header_bottom = slide_header(scene, TITLE, SUBTITLE)
    one_host.record_frame(scene, INSTALL, header_bottom, revision=REVISION)
    note(scene, MARGIN, one_host.NOTE_Y, CLOSING, width=CONTENT_WIDTH)
    return scene
