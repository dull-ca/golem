from __future__ import annotations

from excalidraw.layout import note, slide_header
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene

from . import lichess_stack

SLUG = "lichess-stack"
TITLE = "What lichess runs"

CLOSING_Y = lichess_stack.BOTTOM + 22


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, "What lichess runs")
    lichess_stack.draw(
        scene,
        layer_tones=lichess_stack.DESCRIPTIVE_LAYER_TONES,
        default_part_tone=lichess_stack.DESCRIPTIVE_PART_TONE,
    )
    note(
        scene,
        MARGIN,
        CLOSING_Y,
        "Layer 6 runs across layers 2 to 5, so it is drawn beside them.",
        width=CONTENT_WIDTH,
    )
    return scene
