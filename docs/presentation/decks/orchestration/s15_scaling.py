from __future__ import annotations

from excalidraw import icons
from excalidraw.layout import (
    LabelledBox,
    box_row,
    connector,
    note,
    slide_header,
    span_bar,
)
from excalidraw.palette import BLUE, INK_SOFT, WHITE, WORKLOAD, Tone
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import BODY_SIZE, HEADING_SIZE

from ..vocabulary import PLACEMENT, SCALING, part

SLUG = "scaling"
TITLE = part(SCALING).title

SET_SIZE = 150.0
SET_WIDTH = icons.REPLICA_SET_ASPECT * SET_SIZE

BEFORE_X = 140.0
BEFORE_Y = 330.0

AFTER_X = 1000.0
AFTER_TOP_Y = 240.0
AFTER_BOTTOM_Y = 420.0

ARROW_Y = 405.0
COUNT_Y = 595.0

TRIGGERS_Y = 660.0
TRIGGER_HEIGHT = 150.0
TRIGGER_GAP = 60.0
TRIGGER_WIDTH = (CONTENT_WIDTH - TRIGGER_GAP) / 2.0

CLOSING_Y = 850.0
CLOSING_HEIGHT = 62.0

TRIGGER_TONE = Tone(BLUE, WHITE)

TRIGGERS = (
    LabelledBox("Policy", "you declared a number", TRIGGER_TONE),
    LabelledBox("Load", "a signal moved it", TRIGGER_TONE),
)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        TITLE,
        f"{part(SCALING).detail.capitalize()}.",
    )
    icons.replica_set(scene, BEFORE_X, BEFORE_Y, SET_SIZE, tone=WORKLOAD)
    for top in (AFTER_TOP_Y, AFTER_BOTTOM_Y):
        icons.replica_set(scene, AFTER_X, top, SET_SIZE, tone=WORKLOAD)
    connector(
        scene,
        [(BEFORE_X + SET_WIDTH + 40, ARROW_Y), (AFTER_X - 40, ARROW_Y)],
        stroke=INK_SOFT,
        label="scale out",
        font_size=BODY_SIZE,
    )
    note(scene, BEFORE_X, COUNT_Y, "3 replicas", width=SET_WIDTH, align="center")
    note(scene, AFTER_X, COUNT_Y, "6 replicas", width=SET_WIDTH, align="center")
    box_row(
        scene,
        MARGIN,
        TRIGGERS_Y,
        TRIGGERS,
        box_width=TRIGGER_WIDTH,
        box_height=TRIGGER_HEIGHT,
        gap=TRIGGER_GAP,
        title_font_size=HEADING_SIZE,
        detail_font_size=BODY_SIZE,
    )
    span_bar(
        scene,
        MARGIN,
        CLOSING_Y,
        CONTENT_WIDTH,
        f"Scaling adds replicas. {part(PLACEMENT).title} still has to find each one a node.",
        tone=WORKLOAD,
        height=CLOSING_HEIGHT,
    )
    return scene
