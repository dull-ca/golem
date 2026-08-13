from __future__ import annotations

from typing import Mapping, NamedTuple

from excalidraw import palette
from excalidraw.layout import LabelledBox, badge, box_stack
from excalidraw.palette import INK_GHOST, INK_SOFT, ORANGE, WHITE, Tone
from excalidraw.scene import CONTENT_RIGHT, CONTENT_WIDTH, MARGIN, Scene
from excalidraw.text import LINE_HEIGHT, measured_width
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE, HEADING_SIZE

from ..vocabulary import ORCHESTRATION_PARTS

JOB_TONE = Tone(ORANGE, WHITE)
TOOL_SWATCH_TONE = Tone(INK_SOFT, INK_GHOST)

STACK_Y = 200.0
BOX_HEIGHT = 116.0
BOX_GAP = 16.0
STACK_BOTTOM = STACK_Y + len(ORCHESTRATION_PARTS) * (BOX_HEIGHT + BOX_GAP) - BOX_GAP

RULE_Y = STACK_BOTTOM + 18.0
OUTSIDE_Y = RULE_Y + 18.0
OUTSIDE_HEIGHT = 46.0
OUTSIDE_TEXT_INSET = 62.0
OUTSIDE_TITLE = "Configuration management"

CHIP_RIGHT = MARGIN + CONTENT_WIDTH - 18.0
CHIP_GAP = 16.0
CHIP_MIN_WIDTH = 140.0
CHIP_HEIGHT = BODY_SIZE * LINE_HEIGHT + 16.0
CHIP_CAPTION_GAP = 4.0
HANDOVER_ARROW_LENGTH = 46.0
HANDOVER_ARROW_GAP = 10.0

LEGEND_Y = 78.0
LEGEND_SWATCH_HEIGHT = 30.0
LEGEND_SWATCH_WIDTH = LEGEND_SWATCH_HEIGHT * 1.4
LEGEND_CAPTION_GAP = 12.0
LEGEND_GAP = 48.0
LEGEND_ENTRIES: tuple[tuple[Tone, str, str], ...] = (
    (palette.MANUAL, "dashed", "done by hand"),
    (TOOL_SWATCH_TONE, "solid", "a tool does it"),
)


class Chip(NamedTuple):
    body: str
    tone: Tone
    stroke_style: str = "solid"
    caption: str = ""

    def captioned(self, caption: str) -> "Chip":
        return self._replace(caption=caption)


class Answer(NamedTuple):
    chips: tuple[Chip, ...]
    hands_over: bool


def mixture(*chips: Chip) -> Answer:
    return Answer(chips, hands_over=False)


def decided_then_enacted(decision: Chip, enactment: Chip) -> Answer:
    return Answer((decision, enactment), hands_over=True)


BY_HAND = Chip("by hand", palette.MANUAL, stroke_style="dashed")
ANSIBLE = Chip("Ansible", palette.ANSIBLE)
SYSTEMD = Chip("systemd", palette.SYSTEMD)
GOLEM = Chip("golem", palette.GOLEM)
MONITORING = Chip("monitoring", palette.PLATFORM)
DNSMASQ = Chip("dnsmasq", palette.PLATFORM)
SRV_RECORDS = Chip("SRV records", palette.PLATFORM)


def _draw_chip(scene: Scene, right: float, top: float, chip: Chip) -> dict:
    drawn = badge(
        scene,
        right,
        top,
        chip.body,
        tone=chip.tone,
        font_size=BODY_SIZE,
        anchor="right",
        min_width=CHIP_MIN_WIDTH,
        stroke_style=chip.stroke_style,
    )
    if chip.caption:
        width = measured_width(chip.caption, CAPTION_SIZE)
        scene.text(
            drawn["x"] + (drawn["width"] - width) / 2.0,
            top + CHIP_HEIGHT + CHIP_CAPTION_GAP,
            chip.caption,
            font_size=CAPTION_SIZE,
            colour=INK_SOFT,
        )
    return drawn


def _draw_answer(scene: Scene, top: float, answer: Answer) -> None:
    cursor = CHIP_RIGHT
    for position, chip in enumerate(reversed(answer.chips)):
        cursor = _draw_chip(scene, cursor, top, chip)["x"]
        if position == len(answer.chips) - 1:
            continue
        if answer.hands_over:
            tip = cursor - HANDOVER_ARROW_GAP
            middle = top + CHIP_HEIGHT / 2.0
            scene.arrow(
                [(tip - HANDOVER_ARROW_LENGTH, middle), (tip, middle)],
                stroke=INK_SOFT,
            )
            cursor = tip - HANDOVER_ARROW_LENGTH - HANDOVER_ARROW_GAP
        else:
            cursor -= CHIP_GAP


def _draw_legend(scene: Scene) -> None:
    width = sum(
        LEGEND_SWATCH_WIDTH + LEGEND_CAPTION_GAP + measured_width(caption, CAPTION_SIZE)
        for _, _, caption in LEGEND_ENTRIES
    ) + LEGEND_GAP * (len(LEGEND_ENTRIES) - 1)
    cursor = CONTENT_RIGHT - width
    for tone, stroke_style, caption in LEGEND_ENTRIES:
        scene.rectangle(
            cursor,
            LEGEND_Y,
            LEGEND_SWATCH_WIDTH,
            LEGEND_SWATCH_HEIGHT,
            tone,
            radius=False,
            stroke_width=2,
            stroke_style=stroke_style,
        )
        scene.text(
            cursor + LEGEND_SWATCH_WIDTH + LEGEND_CAPTION_GAP,
            LEGEND_Y + (LEGEND_SWATCH_HEIGHT - CAPTION_SIZE * LINE_HEIGHT) / 2.0,
            caption,
            font_size=CAPTION_SIZE,
            colour=INK_SOFT,
        )
        cursor += (
            LEGEND_SWATCH_WIDTH
            + LEGEND_CAPTION_GAP
            + measured_width(caption, CAPTION_SIZE)
            + LEGEND_GAP
        )


def _draw_outside_row(scene: Scene, held_by: Chip) -> None:
    scene.line(
        [(MARGIN, RULE_Y), (CONTENT_RIGHT, RULE_Y)],
        stroke=INK_GHOST,
        stroke_width=2,
    )
    scene.text(
        MARGIN + OUTSIDE_TEXT_INSET,
        OUTSIDE_Y + (OUTSIDE_HEIGHT - HEADING_SIZE * LINE_HEIGHT) / 2.0,
        OUTSIDE_TITLE,
        font_size=HEADING_SIZE,
        colour=INK_SOFT,
    )
    _draw_chip(
        scene,
        CHIP_RIGHT,
        OUTSIDE_Y + (OUTSIDE_HEIGHT - CHIP_HEIGHT) / 2.0,
        held_by,
    )


def draw(
    scene: Scene,
    answers: Mapping[int, Answer],
    *,
    configuration_management: Chip,
) -> None:
    _draw_legend(scene)
    box_stack(
        scene,
        MARGIN,
        STACK_Y,
        CONTENT_WIDTH,
        [
            LabelledBox(part.title, "", JOB_TONE, index_label=str(part.number))
            for part in ORCHESTRATION_PARTS
        ],
        box_height=BOX_HEIGHT,
        gap=BOX_GAP,
        title_font_size=HEADING_SIZE,
    )
    for position, part in enumerate(ORCHESTRATION_PARTS):
        _draw_answer(
            scene,
            STACK_Y
            + position * (BOX_HEIGHT + BOX_GAP)
            + (BOX_HEIGHT - CHIP_HEIGHT) / 2.0,
            answers[part.number],
        )
    _draw_outside_row(scene, configuration_management)
