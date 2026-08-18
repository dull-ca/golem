from __future__ import annotations

from excalidraw.layout import slide_header
from excalidraw.scene import Scene

from . import one_host

SLUG = "golemd-on-the-host"
TITLE = "golemd, on the host"

SUBTITLE = (
    "golemd is a systemd service. It is started with the host's own name, and "
    "that is the scroll it takes."
)

CAPTION = "a systemd service"


def build() -> Scene:
    scene = Scene(SLUG)
    header_bottom = slide_header(scene, TITLE, SUBTITLE)
    one_host.check_header(header_bottom)
    one_host.draw_box(
        scene,
        one_host.PORTRAIT_Y,
        one_host.PORTRAIT_HEIGHT,
        golemd=True,
        caption=CAPTION,
    )
    return scene
