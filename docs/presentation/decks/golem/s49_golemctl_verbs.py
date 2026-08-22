from __future__ import annotations

from typing import Sequence

from excalidraw.layout import TextLine, note, slide_header, text_card
from excalidraw.palette import (
    BLUE,
    BLUE_FILL,
    INK_SOFT,
    TEAL,
    TEAL_FILL,
    VIOLET,
    VIOLET_FILL,
    Tone,
)
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.text import HAND, MONO
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE

SLUG = "golemctl-verbs"
TITLE = "golemctl — on your machine"

CLIENT_TONE = Tone(BLUE, BLUE_FILL)
FLEET_TONE = Tone(VIOLET, VIOLET_FILL)
WIRE_TONE = Tone(TEAL, TEAL_FILL)

COLUMN_GAP = 40.0
COLUMN_WIDTH = (CONTENT_WIDTH - COLUMN_GAP) / 2.0
VERBS_Y = 190.0
VERB_HEIGHT = 108.0
VERB_PITCH = 124.0

FLEET_Y = 566.0
FLEET_HEIGHT = 152.0

HANDSHAKE_Y = 752.0
HANDSHAKE_HEIGHT = 84.0
HANDSHAKE_GAP = 40.0
HANDSHAKE_WIDTHS = (300.0, 480.0, 380.0)
HANDSHAKE_NOTE_Y = 862.0


def literal(body: str, size: float = BODY_SIZE) -> TextLine:
    return (body, size, MONO)


def gloss(body: str, size: float = CAPTION_SIZE) -> TextLine:
    return (body, size, HAND)


VERBS: tuple[Sequence[TextLine], ...] = (
    (literal("golemctl apply <source> <addr>"), literal("--json  --reattach", CAPTION_SIZE)),
    (
        literal("golemctl plan <source> <addr>"),
        literal("--json  --detail  --against-host", CAPTION_SIZE),
    ),
    (literal("golemctl state <addr>"), gloss("what golemd has applied")),
    (literal("golemctl history <addr>"), gloss("the journal")),
    (literal("golemctl show <addr> <id>"), gloss("one revision")),
)

HANDSHAKE: tuple[str, ...] = (
    "POST /manifest",
    '202 {"reconcile_id": <u64>}',
    "GET /reconciles/:id",
)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, TITLE)
    for position, lines in enumerate(VERBS):
        column = position % 2
        row = position // 2
        text_card(
            scene,
            MARGIN + column * (COLUMN_WIDTH + COLUMN_GAP),
            VERBS_Y + row * VERB_PITCH,
            COLUMN_WIDTH,
            lines,
            CLIENT_TONE,
            height=VERB_HEIGHT,
        )
    text_card(
        scene,
        MARGIN,
        FLEET_Y,
        CONTENT_WIDTH,
        (
            literal("golemctl fleet apply | plan | status"),
            literal("--inventory <PATH>   --hosts <a,b>"),
            gloss("no fleet state, no fleet history, no fleet show", BODY_SIZE),
        ),
        FLEET_TONE,
        height=FLEET_HEIGHT,
    )
    chip_x = MARGIN + 116.0
    chips: list[dict] = []
    for width, body in zip(HANDSHAKE_WIDTHS, HANDSHAKE):
        chips.append(
            text_card(
                scene,
                chip_x,
                HANDSHAKE_Y,
                width,
                (literal(body),),
                WIRE_TONE,
                height=HANDSHAKE_HEIGHT,
                align="center",
            )
        )
        chip_x += width + HANDSHAKE_GAP
    middle = HANDSHAKE_Y + HANDSHAKE_HEIGHT / 2.0
    for position in range(len(chips) - 1):
        scene.arrow(
            [
                (chips[position]["x"] + chips[position]["width"] + 6, middle),
                (chips[position + 1]["x"] - 6, middle),
            ],
            stroke=INK_SOFT,
        )
    note(
        scene,
        MARGIN,
        HANDSHAKE_NOTE_Y,
        "The apply handshake.",
        width=CONTENT_WIDTH,
    )
    return scene
