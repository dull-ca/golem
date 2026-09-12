"""The routes, flags and variants drawn here are quoted from the shipped code.

`FLOW_STEPS` names real `golemctl` flags, the real `PlanScope` and the real
`Observation` variants, so it goes stale when they move. Check against
`sites/website/src/content/docs/reference/cli.mdx` and
`docs/adr/0058-the-plan-reads-the-host-and-only-a-verdict-crosses-the-port.md`,
which is where the claim on the bar across the top comes from.
"""

from __future__ import annotations

from typing import Sequence

from excalidraw.layout import TextLine, slide_header, span_bar, text_card
from excalidraw.palette import (
    BLUE,
    BLUE_FILL,
    GOLEM,
    GREEN,
    GREEN_FILL,
    INK_SOFT,
    TEAL,
    TEAL_FILL,
    Tone,
)
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.text import HAND, MONO
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE

SLUG = "plan-against-host"
TITLE = "Plan before apply"

CLIENT_TONE = Tone(BLUE, BLUE_FILL)
WIRE_TONE = Tone(TEAL, TEAL_FILL)
DAEMON_TONE = Tone(GREEN, GREEN_FILL)

BAR_Y = 190.0
BAR_HEIGHT = 64.0
FLOW_Y = 282.0
FLOW_HEIGHT = 140.0
FLOW_GAP = 22.0


def literal(body: str, size: float = BODY_SIZE) -> TextLine:
    return (body, size, MONO)


def gloss(body: str, size: float = BODY_SIZE) -> TextLine:
    return (body, size, HAND)


FLOW_STEPS: tuple[tuple[Sequence[TextLine], Tone], ...] = (
    (
        (
            literal("golemctl plan --against-host"),
            gloss("the host read is opt-in, from the command line"),
        ),
        CLIENT_TONE,
    ),
    (
        (
            literal("POST /plan?against_host=true"),
            literal("PlanScope = JournalOnly | JournalAndHost", CAPTION_SIZE),
            gloss("without the flag, golemd reads only its journal", CAPTION_SIZE),
        ),
        WIRE_TONE,
    ),
    (
        (
            literal("Reconciler::observe(&[GlyphOp]) -> Observations"),
            gloss("golemd runs dpkg-query and systemctl, and reads the declared paths"),
        ),
        DAEMON_TONE,
    ),
    (
        (
            literal("Observation = Realized | Divergent | Absent | Unknown(Unknowable)"),
            literal("Unknowable = Sealed | Unreadable | NotModelled", CAPTION_SIZE),
            gloss(
                "the verdict crosses the port; the contents stay on the host",
                CAPTION_SIZE,
            ),
        ),
        GOLEM,
    ),
)


def step_y(position: int) -> float:
    return FLOW_Y + position * (FLOW_HEIGHT + FLOW_GAP)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, "Plan before apply")
    span_bar(
        scene,
        MARGIN,
        BAR_Y,
        CONTENT_WIDTH,
        "golemd reads the host and returns a verdict per glyph.",
        tone=GOLEM,
        height=BAR_HEIGHT,
    )
    for position, (lines, tone) in enumerate(FLOW_STEPS):
        text_card(
            scene,
            MARGIN,
            step_y(position),
            CONTENT_WIDTH,
            lines,
            tone,
            height=FLOW_HEIGHT,
        )
    for position in range(len(FLOW_STEPS) - 1):
        scene.arrow(
            [
                (MARGIN + CONTENT_WIDTH / 2.0, step_y(position) + FLOW_HEIGHT + 4),
                (MARGIN + CONTENT_WIDTH / 2.0, step_y(position + 1) - 4),
            ],
            stroke=INK_SOFT,
        )
    return scene
