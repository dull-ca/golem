from __future__ import annotations

from excalidraw.layout import note, slide_header
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene

from . import lichess_stack

SLUG = "lichess-stack"
TITLE = "What lichess runs"


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        "What lichess actually runs",
        "Six layers. Five stack; the sixth is not a layer at all.",
    )
    figure = lichess_stack.draw(
        scene,
        layer_tones=lichess_stack.DESCRIPTIVE_LAYER_TONES,
        default_part_tone=lichess_stack.DESCRIPTIVE_PART_TONE,
    )
    note(
        scene,
        MARGIN,
        figure.bottom + 22,
        "Layer 6 is drawn as a column, not a band, because orchestration acts across "
        "layers 2 to 5 rather than sitting on top of them. Layer 1 is underneath all of it.",
        width=CONTENT_WIDTH,
    )
    return scene
