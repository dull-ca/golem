from __future__ import annotations

from excalidraw.layout import slide_header
from excalidraw.scene import Scene

from . import one_host

SLUG = "one-machine"
TITLE = "One machine"

SUBTITLE = (
    "Everything so far has been thirty machines at once. This is one of them, "
    "before golem."
)


def build() -> Scene:
    scene = Scene(SLUG)
    header_bottom = slide_header(scene, TITLE, SUBTITLE)
    one_host.check_header(header_bottom)
    one_host.draw_box(
        scene, one_host.PORTRAIT_Y, one_host.PORTRAIT_HEIGHT, golemd=False
    )
    return scene
