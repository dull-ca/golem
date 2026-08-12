"""Thirty named machines, each holding the units that actually run on it.

A machine box is not a miniature of the six-layer figure. The six layers are
kinds of *work* — configuring the core OS, running the applications — so they
ride on the tools and the arrows, never inside a host. What sits inside a host
is its units: the services, ingress entries, databases and workloads the Ansible
inventory records for it, in `lichess_fleet.py`.

Two channels, and they are independent on purpose. The **border** says which
tool has done machine-level work on that host. The **cells** say who keeps each
unit — a tool, or a person. A frame can therefore show wide coverage and shallow
depth at once, which is the shape lichess is actually in.

Geometry is constant across the wide frames and is not a parameter: they are
read as one figure changing, and a box that moved between them would read as a
different fleet.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Mapping, NamedTuple, Sequence

from excalidraw.layout import connector
from excalidraw.palette import (
    INK,
    INK_GHOST,
    INK_SOFT,
    MANUAL,
    TRANSPARENT,
    WHITE,
    Tone,
)
from excalidraw.scene import MARGIN, Scene
from excalidraw.text import MONO, measured_height, measured_width, wrapped
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE, HEADING_SIZE

from .lichess_fleet import HOSTS
from .lichess_stack import BAND_LAYERS, DESCRIPTIVE_LAYER_TONES, ORCHESTRATION_LAYER

ID_NAMESPACE = "golem-fleet"

COLUMNS = 6
ROWS = 5

WORK_KEY_X = MARGIN
WORK_KEY_Y = 176.0
WORK_SWATCH = 36.0
WORK_PITCH = 44.0
WORK_NAME_X = WORK_KEY_X + WORK_SWATCH + 12.0
WORK_NAME_WIDTH = 340.0

TOOL_X = MARGIN + 14.0
TOOL_Y = 484.0
TOOL_WIDTH = 336.0
TOOL_HEIGHT = 141.0
TOOL_GAP = 14.0
TOOLS_PER_FRAME = 3
TOOL_PADDING = 14.0
WORK_TAG = 24.0
WORK_TAG_GAP = 6.0

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

LAYER_NAMES: Mapping[int, str] = {
    **{spec.number: spec.title for spec in BAND_LAYERS},
    ORCHESTRATION_LAYER.number: ORCHESTRATION_LAYER.title,
}


class Machine(NamedTuple):
    name: str
    tool_units: int = 0
    hand_units: int = 0
    unknown: bool = False
    keeper: Tone | None = None
    unit_tone: Tone | None = None
    agent: bool = False


class Tool(NamedTuple):
    name: str
    holds: str
    tone: Tone
    work: tuple[int, ...] = ()


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


def baseline_machines(keeper: Tone) -> tuple[Machine, ...]:
    return tuple(Machine(host.name, keeper=keeper) for host in HOSTS)


def units_all_by_hand(keeper: Tone) -> tuple[Machine, ...]:
    return tuple(
        Machine(
            host.name,
            hand_units=host.units,
            unknown=host.unknown,
            keeper=keeper,
        )
        for host in HOSTS
    )


def units_split(keeper: Tone, unit_tone: Tone, **overrides) -> tuple[Machine, ...]:
    return tuple(
        Machine(
            host.name,
            tool_units=host.tool_units,
            hand_units=host.hand_units,
            unknown=host.unknown,
            keeper=unit_tone if host.tool_units else keeper,
            unit_tone=unit_tone,
            agent=bool(host.tool_units) and overrides.get("agents", False),
        )
        for host in HOSTS
    )


def units_arriving(keeper: Tone, unit_tone: Tone, share: float) -> tuple[Machine, ...]:
    return tuple(
        Machine(
            host.name,
            tool_units=round(host.tool_units * share),
            hand_units=host.hand_units + host.tool_units - round(host.tool_units * share),
            unknown=host.unknown,
            keeper=unit_tone if host.tool_units else keeper,
            unit_tone=unit_tone,
            agent=bool(host.tool_units),
        )
        for host in HOSTS
    )


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


def _numbered_swatch(scene: Scene, x: float, y: float, size: float, layer: int) -> None:
    scene.rectangle(x, y, size, size, DESCRIPTIVE_LAYER_TONES[layer], stroke_width=1)
    scene.text(
        x,
        y + (size - CAPTION_SIZE * 1.25) / 2.0,
        str(layer),
        font_size=CAPTION_SIZE,
        colour=INK,
        align="center",
        width=size,
    )


def _draw_work_key(scene: Scene) -> None:
    for position, layer in enumerate(sorted(LAYER_NAMES)):
        top = WORK_KEY_Y + position * WORK_PITCH
        _numbered_swatch(scene, WORK_KEY_X, top, WORK_SWATCH, layer)
        name = wrapped(LAYER_NAMES[layer], WORK_NAME_WIDTH * 0.88, CAPTION_SIZE)
        scene.text(
            WORK_NAME_X,
            top + (WORK_SWATCH - measured_height(name, CAPTION_SIZE)) / 2.0,
            name,
            font_size=CAPTION_SIZE,
            colour=INK_SOFT,
            width=WORK_NAME_WIDTH,
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


def draw(scene: Scene, machines: Sequence[Machine]) -> Fleet:
    _draw_work_key(scene)
    bodies = tuple(
        draw_machine(scene, *machine_origin(index), machine)
        for index, machine in enumerate(machines)
    )
    unit_legend(scene)
    return Fleet(bodies)


def tool_column(scene: Scene, tools: Sequence[Tool]) -> float:
    if len(tools) > TOOLS_PER_FRAME:
        raise ValueError(
            f"the tool column holds {TOOLS_PER_FRAME} chips, not {len(tools)}"
        )
    for position, tool in enumerate(tools):
        top = TOOL_Y + position * (TOOL_HEIGHT + TOOL_GAP)
        scene.rectangle(TOOL_X, top, TOOL_WIDTH, TOOL_HEIGHT, tool.tone)
        text_width = TOOL_WIDTH - 2 * TOOL_PADDING
        scene.text(
            TOOL_X + TOOL_PADDING,
            top + TOOL_PADDING,
            tool.name,
            font_size=BODY_SIZE,
            colour=tool.tone.text,
            width=text_width,
        )
        detail = wrapped(tool.holds, text_width * 0.88, CAPTION_SIZE)
        if detail.count("\n") > 1:
            raise ValueError(f"a tool chip's gloss runs past two lines: {tool.holds!r}")
        scene.text(
            TOOL_X + TOOL_PADDING,
            top + TOOL_PADDING + BODY_SIZE * 1.25 + 6,
            detail,
            font_size=CAPTION_SIZE,
            colour=INK_SOFT,
            width=text_width,
        )
        for slot, layer in enumerate(tool.work):
            _numbered_swatch(
                scene,
                TOOL_X + TOOL_PADDING + slot * (WORK_TAG + WORK_TAG_GAP),
                top + TOOL_HEIGHT - TOOL_PADDING - WORK_TAG,
                WORK_TAG,
                layer,
            )
    span = len(tools) * TOOL_HEIGHT + (len(tools) - 1) * TOOL_GAP
    return TOOL_Y + span / 2.0


def on_one_machine(scene: Scene, chips: int) -> None:
    span = chips * TOOL_HEIGHT + (chips - 1) * TOOL_GAP
    scene.rectangle(
        TOOL_X - 14.0,
        TOOL_Y - 40.0,
        TOOL_WIDTH + 28.0,
        span + 52.0,
        Tone(INK_SOFT, TRANSPARENT),
        stroke_style="dashed",
    )
    scene.text(
        TOOL_X - 6.0,
        TOOL_Y - 34.0,
        "on one laptop, not on the fleet",
        font_size=CAPTION_SIZE,
        colour=INK_SOFT,
        width=TOOL_WIDTH,
    )


def scroll_mark(
    scene: Scene,
    x: float,
    y: float,
    width: float,
    height: float,
    label: str,
    tone: Tone,
    *,
    font_size: float = CAPTION_SIZE,
) -> dict:
    roll = height * 0.16
    body = scene.rectangle(
        x, y + roll, width, height - 2 * roll, Tone(tone.stroke, WHITE), stroke_width=2
    )
    for top in (y, y + height - roll):
        scene.rectangle(x, top, width, roll, tone, stroke_width=2)
    scene.text(
        x,
        y + (height - font_size * 1.25) / 2.0,
        label,
        font_size=font_size,
        colour=tone.stroke,
        align="center",
        font_family=MONO,
        width=width,
    )
    return body


def reaches_the_fleet(scene: Scene, middle_y: float, stroke: str) -> None:
    connector(
        scene,
        [(TOOL_X + TOOL_WIDTH + 10.0, middle_y), (FLEET_X - 10.0, middle_y)],
        stroke=stroke,
        stroke_width=3,
    )
