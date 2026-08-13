from __future__ import annotations

from excalidraw.layout import LabelledBox, badge, box_stack, slide_header
from excalidraw.palette import (
    INK_GHOST,
    INK_SOFT,
    MANUAL,
    ORANGE,
    PLATFORM,
    SYSTEMD,
    WHITE,
    Tone,
)
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.text import measured_width
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE, HEADING_SIZE

from ..vocabulary import HEALTH, LIFECYCLE, ORCHESTRATION_PARTS

SLUG = "what-lichess-uses"
TITLE = "What lichess uses for each"

SUBTITLE = "Four of the five are done by hand."

PART_TONE = Tone(ORANGE, WHITE)
BY_HAND = MANUAL

STACK_Y = 200.0
BOX_HEIGHT = 116.0
BOX_GAP = 16.0
CHIP_RIGHT = MARGIN + CONTENT_WIDTH - 18.0
CHIP_GAP = 16.0
CHIP_MIN_WIDTH = 140.0
LEGEND_Y = 874.0
LEGEND_SWATCH = 30.0
LEGEND_GAP = 48.0

ANSWERS: dict[int, tuple[tuple[str, Tone, str], ...]] = {
    part.number: (("by hand", BY_HAND, "dashed"),) for part in ORCHESTRATION_PARTS
}
ANSWERS[LIFECYCLE] = (("systemd", SYSTEMD, "solid"),)
ANSWERS[HEALTH] = (
    ("by hand", BY_HAND, "dashed"),
    ("monitoring", PLATFORM, "solid"),
    ("systemd", SYSTEMD, "solid"),
)


def _draw_answers(scene: Scene, top: float, number: int) -> None:
    cursor = CHIP_RIGHT
    for body, tone, stroke_style in reversed(ANSWERS[number]):
        chip = badge(
            scene,
            cursor,
            top,
            body,
            tone=tone,
            font_size=BODY_SIZE,
            anchor="right",
            min_width=CHIP_MIN_WIDTH,
            stroke_style=stroke_style,
        )
        cursor -= chip["width"] + CHIP_GAP


def _draw_legend(scene: Scene) -> None:
    entries = (
        (BY_HAND, "dashed", "done by hand"),
        (Tone(INK_SOFT, INK_GHOST), "solid", "a tool does it"),
    )
    cursor = MARGIN
    for tone, stroke_style, caption in entries:
        scene.rectangle(
            cursor,
            LEGEND_Y,
            LEGEND_SWATCH * 1.4,
            LEGEND_SWATCH,
            tone,
            radius=False,
            stroke_width=2,
            stroke_style=stroke_style,
        )
        scene.text(
            cursor + LEGEND_SWATCH * 1.4 + 12,
            LEGEND_Y + (LEGEND_SWATCH - CAPTION_SIZE * 1.25) / 2.0,
            caption,
            font_size=CAPTION_SIZE,
            colour=INK_SOFT,
            width=measured_width(caption, CAPTION_SIZE) + 8,
        )
        cursor += (
            LEGEND_SWATCH * 1.4
            + 12
            + measured_width(caption, CAPTION_SIZE)
            + LEGEND_GAP
        )


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, TITLE, SUBTITLE)
    box_stack(
        scene,
        MARGIN,
        STACK_Y,
        CONTENT_WIDTH,
        [
            LabelledBox(part.title, "", PART_TONE, index_label=str(part.number))
            for part in ORCHESTRATION_PARTS
        ],
        box_height=BOX_HEIGHT,
        gap=BOX_GAP,
        title_font_size=HEADING_SIZE,
    )
    for position, part in enumerate(ORCHESTRATION_PARTS):
        top = STACK_Y + position * (BOX_HEIGHT + BOX_GAP)
        _draw_answers(
            scene,
            top + (BOX_HEIGHT - (BODY_SIZE * 1.25 + 16)) / 2.0,
            part.number,
        )
    _draw_legend(scene)
    return scene
