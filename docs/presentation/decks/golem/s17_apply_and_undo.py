from __future__ import annotations

from typing import Sequence

from excalidraw.layout import TextLine, slide_header, span_bar, text_card
from excalidraw.palette import (
    GOLEM,
    GREEN,
    GREEN_FILL,
    INK_SOFT,
    ORANGE,
    ORANGE_FILL,
    RED,
    RED_FILL,
    Tone,
)
from excalidraw.scene import MARGIN, Scene
from excalidraw.text import HAND, MONO
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE

SLUG = "apply-and-undo"
TITLE = "Inside golemd: apply and undo"

APPLY_TONE = Tone(GREEN, GREEN_FILL)
JOURNAL_TONE = Tone(ORANGE, ORANGE_FILL)
REVERSE_TONE = Tone(RED, RED_FILL)

CARD_X = MARGIN
CARD_WIDTH = 1300.0
CARD_HEIGHT = 152.0
FIRST_CARD_Y = 196.0
CARD_PITCH = 202.0

LOOP_X = 1420.0
CLOSING_Y = 812.0
CLOSING_HEIGHT = 58.0


def literal(body: str, size: float = BODY_SIZE) -> TextLine:
    return (body, size, MONO)


def gloss(body: str, size: float = CAPTION_SIZE) -> TextLine:
    return (body, size, HAND)


CARDS: tuple[tuple[Sequence[TextLine], Tone], ...] = (
    (
        (
            literal("Reconciler::apply(&Glyph, ContentId) -> Outcome"),
            literal("Outcome { op, cid, inverse, changed }"),
            gloss("apply captures the prior state as an Inverse"),
        ),
        APPLY_TONE,
    ),
    (
        (
            literal("Revision { id, created_at, kind, scroll_content_id, outcomes }"),
            literal("kind: RevisionKind = Init | Reconcile"),
            gloss("the append-only journal of what golem actually did"),
        ),
        JOURNAL_TONE,
    ),
    (
        (
            literal("Reconciler::reverse(&Outcome)"),
            gloss("replays that Outcome to restore the prior state exactly"),
        ),
        REVERSE_TONE,
    ),
)


def card_y(position: int) -> float:
    return FIRST_CARD_Y + position * CARD_PITCH


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        "Inside golemd: apply and undo",
        "Every edit writes down how to take itself back.",
    )
    for position, (lines, tone) in enumerate(CARDS):
        text_card(
            scene,
            CARD_X,
            card_y(position),
            CARD_WIDTH,
            lines,
            tone,
            height=CARD_HEIGHT,
        )
    for position in range(len(CARDS) - 1):
        scene.arrow(
            [
                (CARD_X + CARD_WIDTH / 2.0, card_y(position) + CARD_HEIGHT + 8),
                (CARD_X + CARD_WIDTH / 2.0, card_y(position + 1) - 8),
            ],
            stroke=INK_SOFT,
        )
    last_middle = card_y(len(CARDS) - 1) + CARD_HEIGHT / 2.0
    first_middle = FIRST_CARD_Y + CARD_HEIGHT / 2.0
    scene.arrow(
        [
            (CARD_X + CARD_WIDTH + 8, last_middle),
            (LOOP_X, last_middle),
            (LOOP_X, first_middle),
            (CARD_X + CARD_WIDTH + 8, first_middle),
        ],
        stroke=RED,
    )
    span_bar(
        scene,
        MARGIN,
        CLOSING_Y,
        CARD_WIDTH,
        "golem only ever reverses edits it recorded.",
        tone=GOLEM,
        height=CLOSING_HEIGHT,
    )
    return scene
