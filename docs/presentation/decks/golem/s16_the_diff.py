from __future__ import annotations

from excalidraw.layout import (
    TextLine,
    note,
    slide_header,
    span_bar,
    split_compare,
    text_card,
)
from excalidraw.palette import (
    BLUE,
    BLUE_FILL,
    GOLEM,
    INK_SOFT,
    NEUTRAL,
    SLATE,
    SLATE_FILL,
    TEAL,
    TEAL_FILL,
    Tone,
)
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.text import HAND, MONO
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE

SLUG = "the-diff"
TITLE = "Inside golemd: the diff"

PRIOR_TONE = Tone(SLATE, SLATE_FILL)
DESIRED_TONE = Tone(TEAL, TEAL_FILL)
OP_TONE = Tone(BLUE, BLUE_FILL)

PANELS_Y = 190.0
PANELS_HEIGHT = 200.0
PLAN_CARD_Y = 434.0
PLAN_CARD_HEIGHT = 96.0
OPS_Y = 576.0
OPS_HEIGHT = 120.0
OPS_GAP = 30.0
OPS_WIDTH = (CONTENT_WIDTH - 3 * OPS_GAP) / 4.0
CLOSING_Y = 740.0
CLOSING_HEIGHT = 58.0
NOTE_Y = 826.0

SCROLL_LINE = "AddressedScroll { content_id, scroll }"
OPS = ("Install", "Remove", "Replace", "Noop")


def literal(body: str, size: float = BODY_SIZE) -> TextLine:
    return (body, size, MONO)


def gloss(body: str, size: float = CAPTION_SIZE) -> TextLine:
    return (body, size, HAND)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        "Inside golemd: the diff",
        "Two scrolls in, a list of glyph operations out.",
    )
    prior, desired = split_compare(
        scene,
        MARGIN,
        PANELS_Y,
        CONTENT_WIDTH,
        PANELS_HEIGHT,
        ("prior", PRIOR_TONE),
        ("desired", DESIRED_TONE),
    )
    for area, tone in ((prior.body, PRIOR_TONE), (desired.body, DESIRED_TONE)):
        text_card(
            scene,
            area.x,
            area.y,
            area.width,
            (literal(SCROLL_LINE), gloss("golemd selects this host's scroll by name")),
            tone,
        )
    scene.arrow(
        [
            (MARGIN + CONTENT_WIDTH / 2.0, PANELS_Y + PANELS_HEIGHT + 8),
            (MARGIN + CONTENT_WIDTH / 2.0, PLAN_CARD_Y - 8),
        ],
        stroke=INK_SOFT,
    )
    text_card(
        scene,
        MARGIN,
        PLAN_CARD_Y,
        CONTENT_WIDTH,
        (
            literal("reconcile::plan(prior, desired) -> Vec<GlyphOp>"),
            gloss("keyed by Glyph::key()"),
        ),
        NEUTRAL,
        height=PLAN_CARD_HEIGHT,
        align="center",
    )
    for position, operation in enumerate(OPS):
        text_card(
            scene,
            MARGIN + position * (OPS_WIDTH + OPS_GAP),
            OPS_Y,
            OPS_WIDTH,
            (literal(operation),),
            OP_TONE,
            height=OPS_HEIGHT,
            align="center",
        )
    span_bar(
        scene,
        MARGIN,
        CLOSING_Y,
        CONTENT_WIDTH,
        "Four operations. There is no fifth.",
        tone=GOLEM,
        height=CLOSING_HEIGHT,
    )
    note(
        scene,
        MARGIN,
        NOTE_Y,
        "The diff is by content id: the same id means no work.",
        width=CONTENT_WIDTH,
    )
    return scene
