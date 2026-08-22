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
    GOLEM,
    INK,
    INK_SOFT,
    NEUTRAL,
    SLATE,
    SLATE_FILL,
    TEAL,
    TEAL_FILL,
    WHITE,
    Tone,
)
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.text import HAND, MONO
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE

from .glyph_ops import OPS

SLUG = "the-diff"
TITLE = "Inside golemd: the diff"

PRIOR_TONE = Tone(SLATE, SLATE_FILL)
DESIRED_TONE = Tone(TEAL, TEAL_FILL)

OP_MARK = 44.0
OP_MARK_Y = 18.0
OP_NAME_GAP = 12.0

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

# `plan` does not take two scrolls. `prior` is the outcome list golemd journalled
# for the last revision; only `desired` is a scroll — apps/golemd/src/reconcile.rs.
PANELS: tuple[tuple[str, str], ...] = (
    ("&[Outcome]", "what golemd last applied, from the journal"),
    ("&Scroll", "this host's scroll, selected by name from the manifest"),
)


def literal(body: str, size: float = BODY_SIZE) -> TextLine:
    return (body, size, MONO)


def gloss(body: str, size: float = CAPTION_SIZE) -> TextLine:
    return (body, size, HAND)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, "Inside golemd: the diff")
    prior, desired = split_compare(
        scene,
        MARGIN,
        PANELS_Y,
        CONTENT_WIDTH,
        PANELS_HEIGHT,
        ("prior", PRIOR_TONE),
        ("desired", DESIRED_TONE),
    )
    for area, tone, (line, caption) in (
        (prior.body, PRIOR_TONE, PANELS[0]),
        (desired.body, DESIRED_TONE, PANELS[1]),
    ):
        text_card(
            scene,
            area.x,
            area.y,
            area.width,
            (literal(line), gloss(caption)),
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
            literal("reconcile::plan(prior: &[Outcome], desired: &Scroll) -> Vec<GlyphOp>"),
            gloss("keyed by Glyph::key()"),
        ),
        NEUTRAL,
        height=PLAN_CARD_HEIGHT,
        align="center",
    )
    for position, operation in enumerate(OPS):
        left = MARGIN + position * (OPS_WIDTH + OPS_GAP)
        scene.rectangle(
            left,
            OPS_Y,
            OPS_WIDTH,
            OPS_HEIGHT,
            Tone(operation.tone.stroke, WHITE),
        )
        operation.mark(
            scene,
            left + (OPS_WIDTH - OP_MARK) / 2.0,
            OPS_Y + OP_MARK_Y,
            OP_MARK,
            operation.tone,
        )
        scene.text(
            left,
            OPS_Y + OP_MARK_Y + OP_MARK + OP_NAME_GAP,
            operation.name,
            font_size=BODY_SIZE,
            colour=INK,
            font_family=MONO,
            align="center",
            width=OPS_WIDTH,
        )
    span_bar(
        scene,
        MARGIN,
        CLOSING_Y,
        CONTENT_WIDTH,
        "Every glyph golemd compares becomes one of these four operations.",
        tone=GOLEM,
        height=CLOSING_HEIGHT,
    )
    note(
        scene,
        MARGIN,
        NOTE_Y,
        "A glyph whose content id has not changed becomes Noop.",
        width=CONTENT_WIDTH,
    )
    return scene
