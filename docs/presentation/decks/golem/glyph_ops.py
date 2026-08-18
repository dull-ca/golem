from __future__ import annotations

from typing import Callable, NamedTuple

from excalidraw.palette import (
    GOLEM,
    INK_FAINT,
    INK_SOFT,
    RED,
    RED_FILL,
    WHITE,
    YELLOW,
    YELLOW_FILL,
    Tone,
)
from excalidraw.scene import Scene
from excalidraw.text import measured_width
from excalidraw.type_scale import CAPTION_SIZE

MarkDrawer = Callable[..., None]

STROKE_FRACTION = 0.15


def install_mark(scene: Scene, x: float, y: float, size: float, tone: Tone) -> None:
    stroke_width = STROKE_FRACTION * size
    scene.line(
        [(x + 0.50 * size, y + 0.13 * size), (x + 0.50 * size, y + 0.87 * size)],
        stroke=tone.stroke,
        stroke_width=stroke_width,
    )
    scene.line(
        [(x + 0.13 * size, y + 0.50 * size), (x + 0.87 * size, y + 0.50 * size)],
        stroke=tone.stroke,
        stroke_width=stroke_width,
    )


def remove_mark(scene: Scene, x: float, y: float, size: float, tone: Tone) -> None:
    scene.line(
        [(x + 0.11 * size, y + 0.50 * size), (x + 0.89 * size, y + 0.50 * size)],
        stroke=tone.stroke,
        stroke_width=STROKE_FRACTION * size,
    )


def replace_mark(scene: Scene, x: float, y: float, size: float, tone: Tone) -> None:
    scene.line(
        [
            (x + 0.09 * size, y + 0.63 * size),
            (x + 0.33 * size, y + 0.33 * size),
            (x + 0.67 * size, y + 0.67 * size),
            (x + 0.91 * size, y + 0.37 * size),
        ],
        stroke=tone.stroke,
        stroke_width=STROKE_FRACTION * size,
    )


def noop_mark(scene: Scene, x: float, y: float, size: float, tone: Tone) -> None:
    diameter = 0.30 * size
    scene.ellipse(
        x + (size - diameter) / 2.0,
        y + (size - diameter) / 2.0,
        diameter,
        diameter,
        Tone(tone.stroke, tone.stroke),
        stroke_width=1,
    )


class Op(NamedTuple):
    name: str
    verb: str
    tone: Tone
    mark: MarkDrawer

    def units(self, count: int) -> str:
        return f"{self.verb:<9}{count} {'unit' if count == 1 else 'units'}"


INSTALL = Op("Install", "install", GOLEM, install_mark)
REMOVE = Op("Remove", "remove", Tone(RED, RED_FILL), remove_mark)
REPLACE = Op("Replace", "replace", Tone(YELLOW, YELLOW_FILL), replace_mark)
NOOP = Op("Noop", "noop", Tone(INK_FAINT, WHITE, INK_FAINT), noop_mark)

OPS: tuple[Op, ...] = (INSTALL, REMOVE, REPLACE, NOOP)

OP_NAMES: tuple[str, ...] = tuple(op.name for op in OPS)

LEGEND_MARK = 30.0
LEGEND_CAPTION_GAP = 10.0
LEGEND_ENTRY_GAP = 34.0


def op_legend(scene: Scene, x: float, y: float) -> float:
    cursor = x
    for op in OPS:
        op.mark(scene, cursor, y, LEGEND_MARK, op.tone)
        caption_width = measured_width(op.verb, CAPTION_SIZE)
        scene.text(
            cursor + LEGEND_MARK + LEGEND_CAPTION_GAP,
            y + (LEGEND_MARK - CAPTION_SIZE * 1.25) / 2.0,
            op.verb,
            font_size=CAPTION_SIZE,
            colour=INK_SOFT,
            width=caption_width + 8,
        )
        cursor += (
            LEGEND_MARK + LEGEND_CAPTION_GAP + caption_width + LEGEND_ENTRY_GAP
        )
    return cursor
