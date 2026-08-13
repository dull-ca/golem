from __future__ import annotations

from dataclasses import dataclass
from typing import NamedTuple, Sequence

from excalidraw.palette import INK_GHOST, INK_SOFT, MANUAL, WHITE, Tone
from excalidraw.scene import Scene
from excalidraw.text import MONO, measured_width
from excalidraw.type_scale import CAPTION_SIZE, HEADING_SIZE

from .lichess_fleet import HOSTS

COLUMNS = 6
ROWS = 5

FLEET_X = 470.0
FLEET_Y = 176.0
FLEET_WIDTH = 1066.0
MACHINE_GAP_X = 18.0
MACHINE_GAP_Y = 12.0
MACHINE_WIDTH = (FLEET_WIDTH - (COLUMNS - 1) * MACHINE_GAP_X) / COLUMNS
MACHINE_HEIGHT = 117.6
FLEET_BOTTOM = FLEET_Y + ROWS * MACHINE_HEIGHT + (ROWS - 1) * MACHINE_GAP_Y

MACHINE_PADDING = 6.0
NAME_HEIGHT = 24.0
CELL_COLUMNS = 4
CELL_ROWS = 3
CELL_SLOTS = CELL_COLUMNS * CELL_ROWS
CELL_GAP = 4.0

LEGEND_Y = FLEET_BOTTOM + 22.0
LEGEND_SWATCH = 30.0
LEGEND_GAP = 42.0

NOBODY = Tone(INK_GHOST, WHITE)
BY_HAND = MANUAL
UNKNOWN_SIZE = HEADING_SIZE


class Machine(NamedTuple):
    name: str
    tool_units: int = 0
    hand_units: int = 0
    unknown: bool = False
    keeper: Tone | None = None
    unit_tone: Tone | None = None
    agent: bool = False


@dataclass(frozen=True)
class Fleet:
    bodies: tuple[dict, ...]

    def machine(self, index: int) -> dict:
        return self.bodies[index]

    @property
    def bottom(self) -> float:
        return FLEET_BOTTOM


def machine_origin(index: int) -> tuple[float, float]:
    return (
        FLEET_X + (index % COLUMNS) * (MACHINE_WIDTH + MACHINE_GAP_X),
        FLEET_Y + (index // COLUMNS) * (MACHINE_HEIGHT + MACHINE_GAP_Y),
    )


def machine_centre_x(index: int) -> float:
    return machine_origin(index)[0] + MACHINE_WIDTH / 2.0


def index_of(name: str) -> int:
    return next(position for position, host in enumerate(HOSTS) if host.name == name)


def bare_machines() -> tuple[Machine, ...]:
    return tuple(Machine(host.name) for host in HOSTS)


def name_band(font_size: float) -> float:
    return font_size * 1.25 + 6.0


def cell_area(
    x: float,
    y: float,
    width: float,
    height: float,
    name_font_size: float = CAPTION_SIZE,
) -> tuple[float, float, float, float]:
    band = name_band(name_font_size)
    return (
        x + MACHINE_PADDING,
        y + MACHINE_PADDING + band,
        width - 2 * MACHINE_PADDING,
        height - 2 * MACHINE_PADDING - band,
    )


def cell_rect(
    area: tuple[float, float, float, float], slot: int
) -> tuple[float, float, float, float]:
    left, top, width, height = area
    cell_width = (width - (CELL_COLUMNS - 1) * CELL_GAP) / CELL_COLUMNS
    cell_height = (height - (CELL_ROWS - 1) * CELL_GAP) / CELL_ROWS
    return (
        left + (slot % CELL_COLUMNS) * (cell_width + CELL_GAP),
        top + (slot // CELL_COLUMNS) * (cell_height + CELL_GAP),
        cell_width,
        cell_height,
    )


def draw_machine(
    scene: Scene,
    x: float,
    y: float,
    machine: Machine,
    *,
    width: float = MACHINE_WIDTH,
    height: float = MACHINE_HEIGHT,
    name_font_size: float = CAPTION_SIZE,
) -> dict:
    keeper = machine.keeper
    body = scene.rectangle(
        x,
        y,
        width,
        height,
        Tone(keeper.stroke, WHITE) if keeper is not None else NOBODY,
        stroke_width=2,
        stroke_style="solid" if keeper is not None else "dotted",
    )
    scene.text(
        x + MACHINE_PADDING + 2,
        y + MACHINE_PADDING,
        machine.name,
        font_size=name_font_size,
        colour=keeper.stroke if keeper is not None else INK_SOFT,
        font_family=MONO,
        width=width - 2 * MACHINE_PADDING - 4,
    )
    area = cell_area(x, y, width, height, name_font_size)
    unit_tone = machine.unit_tone if machine.unit_tone is not None else keeper
    slot = 0
    for _ in range(machine.tool_units):
        left, top, cell_width, cell_height = cell_rect(area, slot)
        scene.rectangle(
            left,
            top,
            cell_width,
            cell_height,
            unit_tone if unit_tone is not None else NOBODY,
            radius=False,
            stroke_width=2,
        )
        slot += 1
    for _ in range(machine.hand_units):
        left, top, cell_width, cell_height = cell_rect(area, slot)
        scene.rectangle(
            left,
            top,
            cell_width,
            cell_height,
            BY_HAND,
            radius=False,
            stroke_width=2,
            stroke_style="dashed",
        )
        slot += 1
    if machine.unknown:
        _, top, _, cell_height = cell_rect(area, min(slot, CELL_SLOTS - 1))
        rows_used = slot // CELL_COLUMNS
        free_top = area[1] + rows_used * (cell_height + CELL_GAP)
        free_height = area[1] + area[3] - free_top
        scene.text(
            area[0],
            free_top + (free_height - UNKNOWN_SIZE * 1.25) / 2.0,
            "?",
            font_size=UNKNOWN_SIZE,
            colour=BY_HAND.stroke,
            align="center",
            width=area[2],
        )
    if machine.agent:
        golem_mark(scene, x + width - 22.0, y + MACHINE_PADDING, 16.0, unit_tone)
    return body


def golem_mark(scene: Scene, x: float, y: float, size: float, tone: Tone | None) -> None:
    drawn = tone if tone is not None else NOBODY
    scene.rectangle(x, y, size, size * 0.62, drawn, stroke_width=1)
    scene.rectangle(
        x + size * 0.18,
        y + size * 0.68,
        size * 0.64,
        size * 0.32,
        drawn,
        stroke_width=1,
    )


def unit_legend(scene: Scene, x: float = FLEET_X, y: float = LEGEND_Y) -> None:
    entries = (
        (Tone(INK_SOFT, INK_GHOST), "solid", "a unit a tool keeps"),
        (BY_HAND, "dashed", "a unit kept by hand"),
    )
    cursor = x
    for tone, stroke_style, caption in entries:
        scene.rectangle(
            cursor,
            y,
            LEGEND_SWATCH * 1.4,
            LEGEND_SWATCH,
            tone,
            radius=False,
            stroke_width=2,
            stroke_style=stroke_style,
        )
        scene.text(
            cursor + LEGEND_SWATCH * 1.4 + 12,
            y + (LEGEND_SWATCH - CAPTION_SIZE * 1.25) / 2.0,
            caption,
            font_size=CAPTION_SIZE,
            colour=INK_SOFT,
            width=measured_width(caption, CAPTION_SIZE) + 8,
        )
        cursor += LEGEND_SWATCH * 1.4 + 12 + measured_width(caption, CAPTION_SIZE) + LEGEND_GAP
    scene.text(
        cursor,
        y + (LEGEND_SWATCH - UNKNOWN_SIZE * 1.25) / 2.0,
        "?",
        font_size=UNKNOWN_SIZE,
        colour=BY_HAND.stroke,
        width=24,
    )
    scene.text(
        cursor + 30,
        y + (LEGEND_SWATCH - CAPTION_SIZE * 1.25) / 2.0,
        "nobody has it written down",
        font_size=CAPTION_SIZE,
        colour=INK_SOFT,
        width=measured_width("nobody has it written down", CAPTION_SIZE) + 8,
    )


def draw_fleet(scene: Scene, machines: Sequence[Machine]) -> Fleet:
    return Fleet(
        tuple(
            draw_machine(scene, *machine_origin(index), machine)
            for index, machine in enumerate(machines)
        )
    )
