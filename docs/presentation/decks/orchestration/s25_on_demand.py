from __future__ import annotations

from excalidraw import icons
from excalidraw.layout import LabelledBox, badge, labelled_box, note, slide_header
from excalidraw.palette import GAP, GOLEM, INK_GHOST, INK_SOFT, WHITE, Tone
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.text import MONO
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE

SLUG = "on-demand"
TITLE = "golem reconciles when it is told to"

SUBTITLE = "There is no timer on the host, and nothing watches it between applies."

AXIS_Y = 470.0
COLUMN_WIDTH = 452.0
COLUMN_GAP = 58.0
CHIP_Y = 388.0
MARK_Y = 288.0
MARK_SIZE = 78.0
CARD_Y = 520.0
CARD_HEIGHT = 214.0
CLOSING_Y = 790.0

ABSENT = Tone(INK_GHOST, WHITE, INK_SOFT)


def _column_x(position: int) -> float:
    return MARGIN + position * (COLUMN_WIDTH + COLUMN_GAP)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, TITLE, SUBTITLE)
    scene.arrow(
        [(MARGIN, AXIS_Y), (MARGIN + CONTENT_WIDTH, AXIS_Y)],
        stroke=INK_SOFT,
        stroke_width=3,
    )
    scene.text(
        MARGIN + CONTENT_WIDTH - 120.0,
        AXIS_Y - CAPTION_SIZE * 1.25 - 12.0,
        "time",
        font_size=CAPTION_SIZE,
        colour=INK_SOFT,
        align="right",
        width=100.0,
    )

    icons.drift(
        scene,
        _column_x(1) + (COLUMN_WIDTH - icons.DRIFT_ASPECT * MARK_SIZE) / 2.0,
        MARK_Y,
        MARK_SIZE,
    )

    events = (
        ("golemctl apply", GOLEM, MONO),
        ("someone removes a package by hand", GAP, None),
        ("golemctl plan --against-host", GOLEM, MONO),
    )
    for position, (body, tone, family) in enumerate(events):
        centre_x = _column_x(position) + COLUMN_WIDTH / 2.0
        badge(
            scene,
            centre_x,
            CHIP_Y,
            body,
            tone=tone,
            font_size=CAPTION_SIZE,
            anchor="center",
            font_family=family if family is not None else 1,
        )
        scene.line(
            [(centre_x, AXIS_Y - 14.0), (centre_x, AXIS_Y + 14.0)],
            stroke=tone.stroke,
            stroke_width=3,
        )

    cards = (
        (
            LabelledBox(
                "One reconcile, then it stops",
                "golemd diffs its journal against the state it was sent, enacts the "
                "difference, and returns to serving requests",
                GOLEM,
            ),
            "solid",
        ),
        (
            LabelledBox(
                "Nothing happens",
                "no timer, no watcher, no loop: the host stays as the hand left it",
                ABSENT,
            ),
            "dotted",
        ),
        (
            LabelledBox(
                "A verdict per glyph",
                "golemd reads the host and reports realized, divergent, absent or "
                "unknown, and changes nothing",
                GOLEM,
            ),
            "solid",
        ),
    )
    for position, (card, stroke_style) in enumerate(cards):
        labelled_box(
            scene,
            _column_x(position),
            CARD_Y,
            COLUMN_WIDTH,
            CARD_HEIGHT,
            card,
            title_font_size=BODY_SIZE,
            detail_font_size=CAPTION_SIZE,
            align="center",
            stroke_style=stroke_style,
        )
    note(
        scene,
        MARGIN,
        CLOSING_Y,
        "Drift is reported when someone asks for it. golem never corrects it on its own.",
        width=CONTENT_WIDTH,
    )
    return scene
