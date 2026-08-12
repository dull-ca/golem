from __future__ import annotations

from typing import NamedTuple, Sequence

from excalidraw.layout import connector, note, panel, slide_header
from excalidraw.palette import GOLEM, INK_FAINT, MANUAL, NEUTRAL, WHITE, Tone
from excalidraw.scene import (
    CONTENT_WIDTH,
    LABEL_HEADROOM,
    MARGIN,
    Scene,
    fit_width,
)
from excalidraw.text import LINE_HEIGHT, MONO, measured_height, wrapped

SLUG = "what-golem-is"
TITLE = "What golem is, and is not"

PANEL_Y = 142
PANEL_HEIGHT = 156
PANEL_GAP = 32
PANEL_WIDTH = (CONTENT_WIDTH - PANEL_GAP) / 2.0
RIGHT_PANEL_X = MARGIN + PANEL_WIDTH + PANEL_GAP

CAPTION_Y = 318
PAIRING_TOP = 344
PAIRING_HEIGHT = 62
PAIRING_GAP = 8
REQUIREMENT_WIDTH = 640.0
ARROW_GUTTER = 56.0
ANSWER_X = MARGIN + REQUIREMENT_WIDTH + ARROW_GUTTER
ANSWER_WIDTH = CONTENT_WIDTH - REQUIREMENT_WIDTH - ARROW_GUTTER
CLOSING_NOTE_Y = 850

NOT_BODY = (
    "a replacement for bare-metal provisioning, OS installation, or the basics of "
    "networking and security — layer 1 stays where it is"
)

IS_BODY = (
    "a replacement for the custom Python and the new Ansible we were building in "
    "December and January"
)


class Pairing(NamedTuple):
    requirement: str
    answer: str
    token: str = ""


PAIRINGS: tuple[Pairing, ...] = (
    Pairing("describe the state you want", "a declarative program, not a script"),
    Pairing("proper undo", "every edit records its inverse; atomic rollback"),
    Pairing("drop it on any machine", "a small statically linked binary", "golemd"),
    Pairing("assume nothing on the host", "no interpreter, no runtime, no agent stack"),
    Pairing(
        "catch mistakes before the host",
        "a statically typed language and a compiler",
        "emetc",
    ),
    Pairing(
        "see the change before it happens",
        "plan against the live host",
        "golemctl plan --against-host",
    ),
    Pairing(
        "move services safely",
        "reversible revisions, so draining is a real operation",
    ),
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
    font_size: float = 17,
    token_font_size: float = 12,
    padding: float = 14,
) -> dict:
    rect = scene.rectangle(x, y, width, height, GOLEM)
    text_width = width - 2 * padding
    if token:
        token_width = fit_width(token, token_font_size, font_family=MONO)
        token_height = token_font_size * LINE_HEIGHT + 12
        scene.rectangle(
            x + width - padding - token_width,
            y + (height - token_height) / 2.0,
            token_width,
            token_height,
            Tone(GOLEM.stroke, WHITE, GOLEM.stroke),
            label=token,
            label_font_size=token_font_size,
            label_font_family=MONO,
        )
        text_width -= token_width + 16
    laid_out = wrapped(body, text_width * LABEL_HEADROOM, font_size)
    scene.text(
        x + padding,
        y + (height - measured_height(laid_out, font_size)) / 2.0,
        laid_out,
        font_size=font_size,
        colour=GOLEM.text,
        align="center",
        width=text_width,
    )
    return rect


def pairing_rows(
    scene: Scene,
    y: float,
    pairings: Sequence[Pairing],
    *,
    height: float = PAIRING_HEIGHT,
    gap: float = PAIRING_GAP,
) -> list[dict]:
    drawn: list[dict] = []
    for position, pairing in enumerate(pairings):
        top = y + position * (height + gap)
        middle = top + height / 2.0
        scene.rectangle(
            MARGIN,
            top,
            REQUIREMENT_WIDTH,
            height,
            NEUTRAL,
            label=pairing.requirement,
            label_font_size=17,
        )
        connector(
            scene,
            [(MARGIN + REQUIREMENT_WIDTH + 10, middle), (ANSWER_X - 10, middle)],
        )
        drawn.append(
            answer_box(
                scene,
                ANSWER_X,
                top,
                ANSWER_WIDTH,
                height,
                pairing.answer,
                pairing.token,
            )
        )
    return drawn


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        "What golem is, and is not",
        "What it replaces, what it leaves alone, and which requirement each property answers.",
    )
    not_panel = panel(
        scene, MARGIN, PANEL_Y, PANEL_WIDTH, PANEL_HEIGHT, "Not", tone=MANUAL
    )
    note(
        scene,
        not_panel.body.x,
        not_panel.body.y,
        NOT_BODY,
        width=not_panel.body.width,
    )
    is_panel = panel(
        scene, RIGHT_PANEL_X, PANEL_Y, PANEL_WIDTH, PANEL_HEIGHT, "Is", tone=GOLEM
    )
    note(
        scene,
        is_panel.body.x,
        is_panel.body.y,
        IS_BODY,
        width=is_panel.body.width,
    )
    note(
        scene,
        MARGIN,
        CAPTION_Y,
        "what you need",
        width=REQUIREMENT_WIDTH,
        font_size=15,
        colour=INK_FAINT,
    )
    note(
        scene,
        ANSWER_X,
        CAPTION_Y,
        "the property that answers it",
        width=ANSWER_WIDTH,
        font_size=15,
        colour=INK_FAINT,
    )
    pairing_rows(scene, PAIRING_TOP, PAIRINGS)
    note(
        scene,
        MARGIN,
        CLOSING_NOTE_Y,
        "None of this is new orchestration. It is the same work, written down as state "
        "instead of as steps — and reversible when it is wrong.",
        width=CONTENT_WIDTH,
    )
    return scene
