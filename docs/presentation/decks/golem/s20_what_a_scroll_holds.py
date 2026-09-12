from __future__ import annotations

from excalidraw.layout import callout, slide_header
from excalidraw.palette import GOLEM
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene

from . import glyph_kinds, one_host

SLUG = "what-a-scroll-holds"
TITLE = "What a scroll holds"

SUBTITLE = (
    "A scroll is a list of glyphs. A glyph is one thing golem keeps on a host, "
    "and there are four kinds."
)

LIBRARIES = (
    "Workloads, quadlets and ingress are Emet libraries. "
    "They compile down to these four."
)

ROW_X = MARGIN + (CONTENT_WIDTH - glyph_kinds.ROW_WIDTH) / 2.0
ROW_Y = 424.0
CALLOUT_Y = 786.0


def build() -> Scene:
    scene = Scene(SLUG)
    header_bottom = slide_header(scene, TITLE, SUBTITLE)
    one_host.check_header(header_bottom)
    scroll_y = one_host.CONTENT_TOP
    one_host.draw_scroll(scene, scroll_y)
    glyph_kinds.draw_fan(
        scene,
        one_host.SCROLL_CENTRE_X,
        scroll_y + one_host.SCROLL_HEIGHT,
        ROW_X,
        ROW_Y,
    )
    callout(
        scene,
        MARGIN,
        CALLOUT_Y,
        CONTENT_WIDTH,
        LIBRARIES,
        tone=GOLEM,
    )
    return scene
