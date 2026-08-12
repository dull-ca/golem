from __future__ import annotations

from excalidraw.layout import LabelledBox, box_stack, note, slide_header
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene

from . import lichess_stack

SLUG = "orchestration"
TITLE = "What orchestration means"

PARTS_ORIGIN_Y = 176
PART_BOX_HEIGHT = 104
PART_BOX_GAP = 18


def part_boxes() -> list[LabelledBox]:
    return [
        LabelledBox(
            title=part.title,
            detail=part.detail,
            tone=lichess_stack.DESCRIPTIVE_PART_TONE,
            index_label=str(part.number),
        )
        for part in lichess_stack.ORCHESTRATION_PARTS
    ]


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        'What "orchestration" actually means',
        "Layer 6, expanded. One word that is really five separate jobs.",
    )
    drawn = box_stack(
        scene,
        MARGIN,
        PARTS_ORIGIN_Y,
        CONTENT_WIDTH,
        part_boxes(),
        box_height=PART_BOX_HEIGHT,
        gap=PART_BOX_GAP,
        title_font_size=22,
        detail_font_size=15,
        padding=16,
    )
    note(
        scene,
        MARGIN,
        drawn[-1]["y"] + PART_BOX_HEIGHT + 28,
        "None of the five is optional. Every fleet answers all five — by a platform, "
        "by a script, or by a human at a terminal.",
        width=CONTENT_WIDTH,
    )
    return scene
