from __future__ import annotations

from excalidraw import icons
from excalidraw.layout import (
    LabelledBox,
    badge,
    box_stack,
    labelled_box,
    slide_header,
    span_bar,
    split_compare,
)
from excalidraw.palette import GAP, NEUTRAL, NODE, RED, SLATE, WHITE, Tone
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import CAPTION_SIZE

SLUG = "a-process-on-a-host"
TITLE = "A process on a host"

BAND_Y = 190.0
BAND_HEIGHT = 230.0

HOST_X = 116.0
HOST_Y = 218.0
HOST_SIZE = 140.0
HOST_BADGE_Y = 368.0

PROCESS_Y = 232.0
PROCESS_HEIGHT = 150.0
PROCESS_WIDTH = 460.0
FIRST_PROCESS_X = 470.0
SECOND_PROCESS_X = 990.0

PANELS_Y = 452.0
PANELS_HEIGHT = 318.0
ITEM_HEIGHT = 66.0
ITEM_GAP = 14.0

BAR_Y = 802.0
BAR_HEIGHT = 64.0

PROCESS_TONE = Tone(SLATE, WHITE)
GETS_TONE = Tone(SLATE, WHITE)
SHARES_TONE = Tone(RED, WHITE)

PROCESSES = (
    LabelledBox("nginx", "wants libssl 3", PROCESS_TONE),
    LabelledBox("postgres", "wants libssl 1.1", PROCESS_TONE),
)

GETS = ("a PID", "some memory", "the machine's kernel")
SHARES = ("one filesystem", "one network", "one set of library versions")


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        "A process on a host",
        "A running program, sharing one machine with every other program.",
    )
    scene.rectangle(MARGIN, BAND_Y, CONTENT_WIDTH, BAND_HEIGHT, NEUTRAL)
    icons.host(scene, HOST_X, HOST_Y, HOST_SIZE, tone=NODE)
    badge(
        scene,
        HOST_X,
        HOST_BADGE_Y,
        "one host",
        tone=Tone(SLATE, WHITE),
        font_size=CAPTION_SIZE,
    )
    for left, process in zip((FIRST_PROCESS_X, SECOND_PROCESS_X), PROCESSES):
        labelled_box(
            scene,
            left,
            PROCESS_Y,
            PROCESS_WIDTH,
            PROCESS_HEIGHT,
            process,
            align="center",
            detail_colour=RED,
        )
    gets, shares = split_compare(
        scene,
        MARGIN,
        PANELS_Y,
        CONTENT_WIDTH,
        PANELS_HEIGHT,
        ("What it gets", NODE),
        ("What it shares", GAP),
    )
    box_stack(
        scene,
        gets.body.x,
        gets.body.y,
        gets.body.width,
        [LabelledBox(title, "", GETS_TONE) for title in GETS],
        box_height=ITEM_HEIGHT,
        gap=ITEM_GAP,
    )
    box_stack(
        scene,
        shares.body.x,
        shares.body.y,
        shares.body.width,
        [LabelledBox(title, "", SHARES_TONE) for title in SHARES],
        box_height=ITEM_HEIGHT,
        gap=ITEM_GAP,
    )
    span_bar(
        scene,
        MARGIN,
        BAR_Y,
        CONTENT_WIDTH,
        "One filesystem holds one libssl version.",
        tone=GAP,
        height=BAR_HEIGHT,
    )
    return scene
