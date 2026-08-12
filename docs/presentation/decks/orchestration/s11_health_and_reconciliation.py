from __future__ import annotations

from excalidraw import icons
from excalidraw.layout import (
    LabelledBox,
    connector,
    note,
    pipeline,
    slide_header,
    span_bar,
)
from excalidraw.palette import BLUE, GREEN, HEALTHY, INK_FAINT, RED, WHITE, Tone
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import BODY_SIZE

SLUG = "health-and-reconciliation"
TITLE = "Health and reconciliation"

STAGE_GAP = 56.0
STAGE_WIDTH = (CONTENT_WIDTH - 3 * STAGE_GAP) / 4.0
STAGES_Y = 330.0
STAGE_HEIGHT = 180.0

ICON_Y = 200.0
ICON_SIZE = 100.0

OBSERVE = 1
DETECT = 2
ACT = 3

RETURN_Y = 620.0
NOTE_Y = 690.0
CLOSING_Y = 780.0
CLOSING_HEIGHT = 62.0

DESIRED_TONE = Tone(GREEN, WHITE, GREEN)
OBSERVE_TONE = Tone(BLUE, WHITE, BLUE)
DETECT_TONE = Tone(RED, WHITE, RED)
ACT_TONE = Tone(BLUE, WHITE, BLUE)

STAGES = (
    LabelledBox("Desired state", "what you declared", DESIRED_TONE),
    LabelledBox("Observe actual", "what is really running", OBSERVE_TONE),
    LabelledBox("Detect", "drift or failure", DETECT_TONE),
    LabelledBox("Act", "restart or reschedule", ACT_TONE),
)


def stage_centre(position: int) -> float:
    return MARGIN + position * (STAGE_WIDTH + STAGE_GAP) + STAGE_WIDTH / 2.0


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        "Health and reconciliation",
        "Desired against actual, forever.",
    )
    icons.health_probe(
        scene,
        stage_centre(OBSERVE) - icons.HEALTH_PROBE_ASPECT * ICON_SIZE / 2.0,
        ICON_Y,
        ICON_SIZE,
    )
    icons.drift(
        scene,
        stage_centre(DETECT) - icons.DRIFT_ASPECT * ICON_SIZE / 2.0,
        ICON_Y,
        ICON_SIZE,
    )
    pipeline(
        scene,
        MARGIN,
        STAGES_Y,
        STAGES,
        box_width=STAGE_WIDTH,
        box_height=STAGE_HEIGHT,
        gap=STAGE_GAP,
        detail_font_size=BODY_SIZE,
    )
    connector(
        scene,
        [
            (stage_centre(ACT), STAGES_Y + STAGE_HEIGHT + 10),
            (stage_centre(ACT), RETURN_Y),
            (stage_centre(OBSERVE), RETURN_Y),
            (stage_centre(OBSERVE), STAGES_Y + STAGE_HEIGHT + 6),
        ],
        stroke=INK_FAINT,
        label="observe again",
    )
    note(
        scene,
        MARGIN,
        NOTE_Y,
        "A crash and a config change enter the same loop.",
        width=CONTENT_WIDTH,
    )
    span_bar(
        scene,
        MARGIN,
        CLOSING_Y,
        CONTENT_WIDTH,
        'Nothing is ever "done" — the loop is the product.',
        tone=HEALTHY,
        height=CLOSING_HEIGHT,
    )
    return scene
