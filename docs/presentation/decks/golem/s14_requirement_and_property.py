from __future__ import annotations

from typing import NamedTuple, Sequence

from excalidraw.layout import connector, note, slide_header
from excalidraw.palette import GOLEM, INK_FAINT, NEUTRAL, WHITE, Tone
from excalidraw.scene import (
    CONTENT_WIDTH,
    LABEL_HEADROOM,
    MARGIN,
    Scene,
    fit_width,
)
from excalidraw.text import LINE_HEIGHT, MONO, measured_height, wrapped
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE

SLUG = "requirement-and-property"
TITLE = "What you need, and what answers it"

CAPTION_Y = 176.0
ROW_TOP = 212.0
ROW_HEIGHT = 84.0
ROW_GAP = 10.0
REQUIREMENT_WIDTH = 600.0
ARROW_GUTTER = 72.0
ANSWER_X = MARGIN + REQUIREMENT_WIDTH + ARROW_GUTTER
ANSWER_WIDTH = CONTENT_WIDTH - REQUIREMENT_WIDTH - ARROW_GUTTER


class Pairing(NamedTuple):
    requirement: str
    answer: str
    token: str = ""


PAIRINGS: tuple[Pairing, ...] = (
    Pairing("describe the state you want", "a declarative program, not a script"),
    Pairing("proper undo", "every edit records its inverse"),
    Pairing("drop it on any machine", "a small statically linked binary", "golemd"),
    Pairing("assume nothing on the host", "no interpreter, no runtime, no agent"),
    Pairing(
        "catch mistakes before the host", "a statically typed compiler", "emetc"
    ),
    Pairing(
        "see the change before it happens",
        "plan against the live host",
        "golemctl plan --against-host",
    ),
    Pairing("move services safely", "reversible revisions, so drain is real"),
)


def answer_box(
    scene: Scene,
    x: float,
    y: float,
    width: float,
    height: float,
    body: str,
    token: str,
    *,
    padding: float = 16.0,
) -> dict:
    rect = scene.rectangle(x, y, width, height, GOLEM)
    text_width = width - 2 * padding
    if token:
        token_width = fit_width(token, CAPTION_SIZE, font_family=MONO)
        token_height = CAPTION_SIZE * LINE_HEIGHT + 14
        scene.rectangle(
            x + width - padding - token_width,
            y + (height - token_height) / 2.0,
            token_width,
            token_height,
            Tone(GOLEM.stroke, WHITE, GOLEM.stroke),
            label=token,
            label_font_size=CAPTION_SIZE,
            label_font_family=MONO,
        )
        text_width -= token_width + 18
    laid_out = wrapped(body, text_width * LABEL_HEADROOM, BODY_SIZE)
    scene.text(
        x + padding,
        y + (height - measured_height(laid_out, BODY_SIZE)) / 2.0,
        laid_out,
        font_size=BODY_SIZE,
        colour=GOLEM.text,
        align="center",
        width=text_width,
    )
    return rect


def pairing_rows(scene: Scene, y: float, pairings: Sequence[Pairing]) -> list[dict]:
    drawn: list[dict] = []
    for position, pairing in enumerate(pairings):
        top = y + position * (ROW_HEIGHT + ROW_GAP)
        middle = top + ROW_HEIGHT / 2.0
        scene.rectangle(
            MARGIN,
            top,
            REQUIREMENT_WIDTH,
            ROW_HEIGHT,
            NEUTRAL,
            label=pairing.requirement,
            label_font_size=BODY_SIZE,
        )
        connector(
            scene,
            [(MARGIN + REQUIREMENT_WIDTH + 12, middle), (ANSWER_X - 12, middle)],
        )
        drawn.append(
            answer_box(
                scene,
                ANSWER_X,
                top,
                ANSWER_WIDTH,
                ROW_HEIGHT,
                pairing.answer,
                pairing.token,
            )
        )
    return drawn


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        "What you need, and what answers it",
        "Every row is a debt from the last three slides being paid.",
    )
    note(
        scene,
        MARGIN,
        CAPTION_Y,
        "what you need",
        width=REQUIREMENT_WIDTH,
        font_size=CAPTION_SIZE,
        colour=INK_FAINT,
    )
    note(
        scene,
        ANSWER_X,
        CAPTION_Y,
        "the property that answers it",
        width=ANSWER_WIDTH,
        font_size=CAPTION_SIZE,
        colour=INK_FAINT,
    )
    pairing_rows(scene, ROW_TOP, PAIRINGS)
    return scene
