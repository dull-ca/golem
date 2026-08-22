from __future__ import annotations

from excalidraw.layout import LabelledBox, box_stack, note, slide_header
from excalidraw.palette import ORANGE, WHITE, Tone
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import BODY_SIZE, HEADING_SIZE

from ..vocabulary import ORCHESTRATION_PARTS

SLUG = "the-five-jobs"
TITLE = "The five jobs"

PART_TONE = Tone(ORANGE, WHITE)

STACK_Y = 200.0
BOX_HEIGHT = 116.0
BOX_GAP = 16.0
CLOSING_Y = 866.0


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, "The five jobs a cluster has to do")
    box_stack(
        scene,
        MARGIN,
        STACK_Y,
        CONTENT_WIDTH,
        [
            LabelledBox(part.title, part.detail, PART_TONE, index_label=str(part.number))
            for part in ORCHESTRATION_PARTS
        ],
        box_height=BOX_HEIGHT,
        gap=BOX_GAP,
        title_font_size=HEADING_SIZE,
        detail_font_size=BODY_SIZE,
    )
    note(
        scene,
        MARGIN,
        CLOSING_Y,
        "Every fleet does all five, by a platform, by a script, or by a person.",
        width=CONTENT_WIDTH,
    )
    return scene
