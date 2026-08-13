from __future__ import annotations

from excalidraw.layout import slide_header
from excalidraw.palette import THEIRS, WHITE, YOURS, Tone
from excalidraw.scene import Scene

from . import stack

SLUG = "the-stack"
TITLE = "The stack, and where you take over"

SUBTITLE = "A provider sells you the bottom three. Everything above them is yours to configure."

BOUGHT_TONE = THEIRS
CONFIGURED_TONE = YOURS


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, TITLE, SUBTITLE)
    stack.draw(
        scene,
        band_tones={
            **{number: BOUGHT_TONE for number in stack.BOUGHT_BANDS},
            **{number: CONFIGURED_TONE for number in stack.CONFIGURED_BANDS},
        },
        column_tone=CONFIGURED_TONE,
        default_part_tone=Tone(CONFIGURED_TONE.stroke, WHITE),
    )
    stack.gutter_bar(
        scene,
        (0, stack.LANES - 1),
        (min(stack.CONFIGURED_BANDS), max(stack.CONFIGURED_BANDS)),
        "What you configure",
        CONFIGURED_TONE,
        detail="every one of these is a decision you make and have to keep making",
    )
    stack.gutter_bar(
        scene,
        (0, stack.LANES - 1),
        (min(stack.BOUGHT_BANDS), max(stack.BOUGHT_BANDS)),
        "What you buy",
        BOUGHT_TONE,
        detail="one invoice, and someone else operates it",
    )
    return scene
