from __future__ import annotations

from excalidraw import icons
from excalidraw.layout import LabelledBox, box_row, note, slide_header, span_bar
from excalidraw.palette import CONTROL, GREEN, PENDING, SLATE, WHITE, Tone
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import BODY_SIZE, HEADING_SIZE

from ..vocabulary import PLACEMENT, part

SLUG = "placement-the-binding"
TITLE = f"{part(PLACEMENT).title}: the binding"

BINDING_X = 640.0
BINDING_Y = 210.0
BINDING_SIZE = 300.0

SCHEDULER_X = 170.0
SCHEDULER_Y = 280.0
SCHEDULER_SIZE = 160.0
SCHEDULER_CAPTION_Y = 458.0
SCHEDULER_CAPTION_BLEED = 40.0

MOMENTS_Y = 570.0
MOMENT_HEIGHT = 180.0
MOMENT_GAP = 36.0
MOMENT_WIDTH = (CONTENT_WIDTH - 2 * MOMENT_GAP) / 3.0

CLOSING_Y = 820.0
CLOSING_HEIGHT = 62.0

CANDIDATE_TONE = Tone(SLATE, WHITE, SLATE)
BOUND_TONE = Tone(GREEN, WHITE, GREEN)

MOMENTS = (
    LabelledBox("Pending", "nothing is running yet", PENDING),
    LabelledBox("Candidates", "every node that could take it", CANDIDATE_TONE),
    LabelledBox("Bound", "one node, the runtime's job now", BOUND_TONE),
)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        TITLE,
        "A binding is the record that this workload runs on that node.",
    )
    icons.scheduler(scene, SCHEDULER_X, SCHEDULER_Y, SCHEDULER_SIZE)
    note(
        scene,
        SCHEDULER_X - SCHEDULER_CAPTION_BLEED,
        SCHEDULER_CAPTION_Y,
        "the scheduler",
        width=icons.SCHEDULER_ASPECT * SCHEDULER_SIZE + 2 * SCHEDULER_CAPTION_BLEED,
        font_size=BODY_SIZE,
        align="center",
    )
    icons.binding(scene, BINDING_X, BINDING_Y, BINDING_SIZE)
    box_row(
        scene,
        MARGIN,
        MOMENTS_Y,
        MOMENTS,
        box_width=MOMENT_WIDTH,
        box_height=MOMENT_HEIGHT,
        gap=MOMENT_GAP,
        title_font_size=HEADING_SIZE,
        detail_font_size=BODY_SIZE,
    )
    span_bar(
        scene,
        MARGIN,
        CLOSING_Y,
        CONTENT_WIDTH,
        "Placement is the only part that chooses a node.",
        tone=CONTROL,
        height=CLOSING_HEIGHT,
    )
    return scene
