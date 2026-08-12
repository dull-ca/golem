from __future__ import annotations

from excalidraw import icons
from excalidraw.layout import IconCard, icon_card_row, note, slide_header, span_bar
from excalidraw.palette import GAP, MANUAL
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import BODY_SIZE, HEADING_SIZE

SLUG = "december-gaps"
TITLE = "December: what it could not do"

CARDS_Y = 220.0
CARD_HEIGHT = 370.0
ICON_SIZE = 150.0
CARD_GAP = 40.0
NOTE_Y = 630.0
CLOSING_Y = 700.0
CLOSING_HEIGHT = 58.0

CARDS = (
    IconCard(
        icons.drain,
        icons.DRAIN_ASPECT,
        "Drain",
        "nothing moved work off a host first",
        MANUAL,
    ),
    IconCard(
        icons.binding,
        icons.BINDING_ASPECT,
        "Move a service",
        "placement changed only by editing the table",
        MANUAL,
    ),
    IconCard(
        icons.rollback,
        icons.ROLLBACK_ASPECT,
        "Roll back",
        "nothing recorded the state before",
        MANUAL,
    ),
)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        "Three things December could not do",
        "Not bugs. Missing operations.",
    )
    icon_card_row(
        scene,
        MARGIN,
        CARDS_Y,
        CARDS,
        card_height=CARD_HEIGHT,
        icon_size=ICON_SIZE,
        gap=CARD_GAP,
        title_font_size=HEADING_SIZE,
        detail_font_size=BODY_SIZE,
    )
    note(
        scene,
        MARGIN,
        NOTE_Y,
        "Nothing on the host knew what it was supposed to look like.",
        width=CONTENT_WIDTH,
    )
    span_bar(
        scene,
        MARGIN,
        CLOSING_Y,
        CONTENT_WIDTH,
        "Placement lived in a Python file and in a human's head.",
        tone=GAP,
        height=CLOSING_HEIGHT,
    )
    return scene
