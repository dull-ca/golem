"""What the golem deck adds to the shared fleet: the six kinds of work.

The machine box and the fleet layout are in `decks/machines.py`, which both
decks draw from. What is here is the vocabulary only this deck speaks — the six
layers as *kinds of work* rather than strata inside a machine, the tool chips
that name who does which of them, and the per-frame machine states the sequence
steps through.

A layer is an activity a tool performs on a machine, so the layer tones ride on
the chips and the arrows and never inside a host. That is what lets one frame
say Ansible touches all thirty machines and keeps the units on eight.
"""

from __future__ import annotations

from typing import Mapping, NamedTuple, Sequence

from excalidraw.layout import connector
from excalidraw.palette import INK, INK_SOFT, TRANSPARENT, WHITE, Tone
from excalidraw.scene import MARGIN, Scene
from excalidraw.text import MONO, measured_height, wrapped
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE

from ..lichess_fleet import HOSTS
from ..machines import (
    FLEET_X,
    MACHINE_HEIGHT,
    MACHINE_WIDTH,
    Fleet,
    Machine,
    bare_machines,
    baseline_machines,
    cell_area,
    cell_rect,
    draw_fleet,
    draw_machine,
    machine_origin,
    scroll_mark,
    unit_legend,
)
from .lichess_stack import BAND_LAYERS, DESCRIPTIVE_LAYER_TONES, ORCHESTRATION_LAYER

__all__ = [
    "ID_NAMESPACE",
    "MACHINE_HEIGHT",
    "MACHINE_WIDTH",
    "Fleet",
    "Machine",
    "Tool",
    "bare_machines",
    "baseline_machines",
    "cell_area",
    "cell_rect",
    "draw",
    "draw_fleet",
    "draw_machine",
    "machine_origin",
    "on_one_machine",
    "reaches_the_fleet",
    "scroll_mark",
    "tool_column",
    "units_all_by_hand",
    "units_arriving",
    "units_split",
]

ID_NAMESPACE = "golem-fleet"

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

LAYER_NAMES: Mapping[int, str] = {
    **{spec.number: spec.title for spec in BAND_LAYERS},
    ORCHESTRATION_LAYER.number: ORCHESTRATION_LAYER.title,
}


class Tool(NamedTuple):
    name: str
    holds: str
    tone: Tone
    work: tuple[int, ...] = ()


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


def draw(scene: Scene, machines: Sequence[Machine]) -> Fleet:
    _draw_work_key(scene)
    fleet = draw_fleet(scene, machines)
    unit_legend(scene)
    return fleet


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


def reaches_the_fleet(scene: Scene, middle_y: float, stroke: str) -> None:
    connector(
        scene,
        [(TOOL_X + TOOL_WIDTH + 10.0, middle_y), (FLEET_X - 10.0, middle_y)],
        stroke=stroke,
        stroke_width=3,
    )
