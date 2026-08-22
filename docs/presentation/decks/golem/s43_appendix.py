from __future__ import annotations

from excalidraw.layout import note
from excalidraw.palette import INK_GHOST, INK_SOFT
from excalidraw.scene import CANVAS_HEIGHT, CONTENT_RIGHT, CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import TITLE_SIZE

SLUG = "appendix"
TITLE = "Appendix"

RULE_Y = CANVAS_HEIGHT / 2.0
WORD_Y = RULE_Y + 36.0


def build() -> Scene:
    scene = Scene(SLUG)
    scene.line(
        [(MARGIN, RULE_Y), (CONTENT_RIGHT, RULE_Y)],
        stroke=INK_GHOST,
        stroke_width=2,
    )
    note(
        scene,
        MARGIN,
        WORD_Y,
        TITLE,
        width=CONTENT_WIDTH,
        font_size=TITLE_SIZE,
        colour=INK_SOFT,
        align="center",
    )
    return scene
