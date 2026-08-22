from __future__ import annotations

from excalidraw import icons
from excalidraw.layout import LabelledBox, connector, labelled_box, note, slide_header
from excalidraw.palette import BLUE, GREEN, INK_FAINT, RED, WHITE, Tone
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import BODY_SIZE

from ..vocabulary import HEALTH, part

SLUG = "health-and-reconciliation"
TITLE = part(HEALTH).title

STAGE_WIDTH = 430.0
STAGE_HEIGHT = 150.0

LEFT_X = 140.0
RIGHT_X = 1010.0
TOP_Y = 220.0
BOTTOM_Y = 540.0

DESIRED_TONE = Tone(GREEN, WHITE, GREEN)
OBSERVE_TONE = Tone(BLUE, WHITE, BLUE)
DETECT_TONE = Tone(RED, WHITE, RED)
ACT_TONE = Tone(BLUE, WHITE, BLUE)

DESIRED = LabelledBox("Desired state", "what you declared", DESIRED_TONE)
OBSERVE = LabelledBox("Observe actual", "what is really running", OBSERVE_TONE)
DETECT = LabelledBox("Detect", "drift or failure", DETECT_TONE)
ACT = LabelledBox("Act", "restart or reschedule", ACT_TONE)

ICON_SIZE = 100.0
PROBE_X = 620.0
DRIFT_X = 810.0
ICON_Y = 405.0

NOTE_Y = 740.0


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        TITLE,
        "Reconciliation compares the state you asked for against the state on the "
        "host, and acts on the difference.",
    )
    for x, y, box in (
        (LEFT_X, TOP_Y, DESIRED),
        (RIGHT_X, TOP_Y, OBSERVE),
        (RIGHT_X, BOTTOM_Y, DETECT),
        (LEFT_X, BOTTOM_Y, ACT),
    ):
        labelled_box(
            scene,
            x,
            y,
            STAGE_WIDTH,
            STAGE_HEIGHT,
            box,
            detail_font_size=BODY_SIZE,
            align="center",
        )
    top_middle = TOP_Y + STAGE_HEIGHT / 2.0
    bottom_middle = BOTTOM_Y + STAGE_HEIGHT / 2.0
    left_middle = LEFT_X + STAGE_WIDTH / 2.0
    right_middle = RIGHT_X + STAGE_WIDTH / 2.0
    connector(
        scene,
        [(LEFT_X + STAGE_WIDTH + 10, top_middle), (RIGHT_X - 10, top_middle)],
        stroke=INK_FAINT,
        label="every few seconds",
    )
    connector(
        scene,
        [(right_middle, TOP_Y + STAGE_HEIGHT + 10), (right_middle, BOTTOM_Y - 10)],
        stroke=INK_FAINT,
    )
    connector(
        scene,
        [(RIGHT_X - 10, bottom_middle), (LEFT_X + STAGE_WIDTH + 10, bottom_middle)],
        stroke=INK_FAINT,
    )
    connector(
        scene,
        [(left_middle, BOTTOM_Y - 10), (left_middle, TOP_Y + STAGE_HEIGHT + 10)],
        stroke=INK_FAINT,
    )
    icons.health_probe(scene, PROBE_X, ICON_Y, ICON_SIZE)
    icons.drift(scene, DRIFT_X, ICON_Y, ICON_SIZE)
    note(
        scene,
        MARGIN,
        NOTE_Y,
        "A crash and a config change enter the same loop.",
        width=CONTENT_WIDTH,
    )
    return scene
