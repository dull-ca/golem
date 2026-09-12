from __future__ import annotations

from typing import Sequence

from excalidraw.layout import TextLine, slide_header, text_card
from excalidraw.palette import (
    BLUE,
    BLUE_FILL,
    GREEN,
    GREEN_FILL,
    INK_SOFT,
    SLATE,
    SLATE_FILL,
    TEAL,
    TEAL_FILL,
    VIOLET,
    VIOLET_FILL,
    Tone,
)
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.text import HAND, MONO
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE

SLUG = "the-pipeline"
TITLE = "The pipeline"

SOURCE_TONE = Tone(SLATE, SLATE_FILL)
COMPILER_TONE = Tone(VIOLET, VIOLET_FILL)
MANIFEST_TONE = Tone(TEAL, TEAL_FILL)
CLIENT_TONE = Tone(BLUE, BLUE_FILL)
DAEMON_TONE = Tone(GREEN, GREEN_FILL)

STAGE_Y = 200.0
STAGE_HEIGHT = 200.0
STAGE_GAP = 40.0
STAGE_WIDTH = (CONTENT_WIDTH - 4 * STAGE_GAP) / 5.0

FACTS_Y = 470.0


def literal(body: str, size: float = BODY_SIZE) -> TextLine:
    return (body, size, MONO)


def gloss(body: str, size: float = CAPTION_SIZE) -> TextLine:
    return (body, size, HAND)


STAGES: tuple[tuple[Sequence[TextLine], Tone], ...] = (
    ((literal("fleet.emet"), gloss("the program you author")), SOURCE_TONE),
    ((literal("emetc build"), gloss("one compile for the fleet")), COMPILER_TONE),
    ((literal("manifest"), gloss("binary, content-addressed")), MANIFEST_TONE),
    ((literal("golemctl apply"), gloss("POST /manifest", CAPTION_SIZE)), CLIENT_TONE),
    ((literal("golemd"), gloss("on the host")), DAEMON_TONE),
)

MANIFEST_FACTS: tuple[TextLine, ...] = (
    gloss("The manifest:", BODY_SIZE),
    literal("Manifest { format_version, emet_version, scrolls: Vec<AddressedScroll> }"),
    literal("AddressedScroll { content_id, scroll }        FORMAT_VERSION = 5"),
    literal("ContentId = 32-byte BLAKE3 over postcard bytes, per scroll and per glyph"),
)


def stage_x(position: int) -> float:
    return MARGIN + position * (STAGE_WIDTH + STAGE_GAP)


def draw_stages(scene: Scene) -> None:
    for position, (lines, tone) in enumerate(STAGES):
        text_card(
            scene,
            stage_x(position),
            STAGE_Y,
            STAGE_WIDTH,
            lines,
            tone,
            height=STAGE_HEIGHT,
            align="center",
        )
    middle = STAGE_Y + STAGE_HEIGHT / 2.0
    for position in range(len(STAGES) - 1):
        scene.arrow(
            [
                (stage_x(position) + STAGE_WIDTH + 8, middle),
                (stage_x(position + 1) - 8, middle),
            ],
            stroke=INK_SOFT,
        )


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        "The pipeline",
        "Each host diffs its own scroll from one manifest.",
    )
    draw_stages(scene)
    text_card(scene, MARGIN, FACTS_Y, CONTENT_WIDTH, MANIFEST_FACTS, MANIFEST_TONE)
    return scene
