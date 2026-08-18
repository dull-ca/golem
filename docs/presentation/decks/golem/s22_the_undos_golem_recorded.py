from __future__ import annotations

from excalidraw.layout import note, slide_header
from excalidraw.palette import RED
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene

from . import exemplar, one_host
from .glyph_ops import INSTALL

SLUG = "the-undos-golem-recorded"
TITLE = "The second apply: the undos golem recorded"

SUBTITLE = (
    "The next scroll no longer names the package. golem runs the inverse it "
    "wrote down."
)

REVISION = "revision 1"

CLOSING = (
    "Reverse runs apt-get remove, not purge — configuration the package wrote "
    "under /etc stays."
)


def build() -> Scene:
    scene = Scene(SLUG)
    header_bottom = slide_header(scene, TITLE, SUBTITLE)
    one_host.record_frame(
        scene,
        INSTALL,
        header_bottom,
        revision=REVISION,
        replayed=exemplar.WITHDRAWN,
    )
    note(
        scene,
        MARGIN,
        one_host.NOTE_Y,
        CLOSING,
        width=CONTENT_WIDTH,
        colour=RED,
    )
    return scene
