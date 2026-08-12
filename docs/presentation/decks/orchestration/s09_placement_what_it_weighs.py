from __future__ import annotations

from excalidraw import icons
from excalidraw.layout import LabelledBox, hub_and_satellites, note, slide_header
from excalidraw.palette import BLUE, CONTROL, WHITE, Tone
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import BODY_SIZE

from ..vocabulary import part

SLUG = "placement-what-it-weighs"
TITLE = "What the scheduler weighs"

HUB_WIDTH = 380.0
SCHEDULER_SIZE = 150.0
SCHEDULER_X = MARGIN + (HUB_WIDTH - icons.SCHEDULER_ASPECT * SCHEDULER_SIZE) / 2.0
SCHEDULER_Y = 222.0

SATELLITES_Y = 215.0
SATELLITE_HEIGHT = 140.0
SATELLITE_GAP = 18.0

CLOSING_Y = 862.0

INPUT_TONE = Tone(BLUE, WHITE)

INPUTS = (
    LabelledBox("Resources", "CPU, memory, disk asked for versus free", INPUT_TONE),
    LabelledBox("Constraints", "must run here, must not run there", INPUT_TONE),
    LabelledBox("Affinity", "keep these together, those apart", INPUT_TONE),
    LabelledBox("Spread", "not every replica on one node", INPUT_TONE),
)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        "What the scheduler weighs",
        f"{part(1).title}, up close. Four inputs, one node.",
    )
    icons.scheduler(scene, SCHEDULER_X, SCHEDULER_Y, SCHEDULER_SIZE)
    hub_and_satellites(
        scene,
        MARGIN,
        SATELLITES_Y,
        CONTENT_WIDTH,
        LabelledBox("Scheduler", "one decision", CONTROL),
        INPUTS,
        satellite_height=SATELLITE_HEIGHT,
        hub_width=HUB_WIDTH,
        gap=SATELLITE_GAP,
        satellite_detail_font_size=BODY_SIZE,
    )
    note(
        scene,
        MARGIN,
        CLOSING_Y,
        "A node that fails a constraint is not a candidate.",
        width=CONTENT_WIDTH,
    )
    return scene
