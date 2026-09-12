from __future__ import annotations

from excalidraw.layout import LabelledBox, box_stack, note, slide_header
from excalidraw.palette import MANUAL
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import BODY_SIZE, HEADING_SIZE

SLUG = "december-moving-a-service"
TITLE = "December: moving a service"

STEPS_Y = 190.0
STEP_HEIGHT = 118.0
STEP_GAP = 16.0

FAILURE_NOTE_Y = 724.0

STEPS = (
    LabelledBox(
        "Edit the definition", "mark the service disabled", MANUAL, index_label="1"
    ),
    LabelledBox("Apply to host A", "it stops and uninstalls", MANUAL, index_label="2"),
    LabelledBox(
        "Edit again", "remove from host A, add to host B", MANUAL, index_label="3"
    ),
    LabelledBox("Apply", "it installs on host B", MANUAL, index_label="4"),
)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, TITLE)
    box_stack(
        scene,
        MARGIN,
        STEPS_Y,
        CONTENT_WIDTH,
        STEPS,
        box_height=STEP_HEIGHT,
        gap=STEP_GAP,
        title_font_size=HEADING_SIZE,
        detail_font_size=BODY_SIZE,
    )
    note(
        scene,
        MARGIN,
        FAILURE_NOTE_Y,
        "Out of order, it runs on both hosts or on neither.",
        width=CONTENT_WIDTH,
    )
    return scene
