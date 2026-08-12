"""Twenty-four machines, drawn once, with each frame supplying only their state.

Every machine is a miniature of the six-layer figure in `lichess_stack.py`: layer
1 full width along the bottom, layers 2 to 5 stacked above it, layer 6 as a column
beside them. The tones are that module's `DESCRIPTIVE_LAYER_TONES`, imported
rather than restated, so a machine box and slide 05 can never drift apart and a
viewer can read a box by colour alone. The key on the left carries the numbers.

An unconfigured layer is drawn dashed and empty. Layer 1's slate fill is pale
enough that a solid outline alone did not separate "Ansible has been here" from
"nothing has", which is the whole delta of the second frame.

Geometry is constant across the six frames and is not a parameter: the frames are
read as one figure changing, and a box that moved between them would read as a
different fleet. `draw()` emits the key, the machines and the Portainer badge in a
fixed order and with a fixed element count, so those ids hold across the sequence
under the shared namespace in `ID_NAMESPACE`.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Mapping, NamedTuple, Sequence

from excalidraw.layout import LabelledBox, badge, connector, labelled_box
from excalidraw.palette import BESPOKE, GOLEM, INK_FAINT, INK_SOFT, WHITE, Tone
from excalidraw.scene import MARGIN, Scene
from excalidraw.text import measured_height, wrapped
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE

from .lichess_stack import BAND_LAYERS, DESCRIPTIVE_LAYER_TONES, ORCHESTRATION_LAYER

ID_NAMESPACE = "golem-fleet"

COLUMNS = 6
ROWS = 4
MACHINE_COUNT = COLUMNS * ROWS

KEY_X = MARGIN
KEY_Y = 190.0
KEY_SWATCH = 40.0
KEY_PITCH = 53.0
KEY_NAME_X = KEY_X + KEY_SWATCH + 12.0
KEY_NAME_WIDTH = 248.0

TOOL_X = MARGIN
TOOL_Y = 526.0
TOOL_WIDTH = 300.0
TOOL_HEIGHT = 100.0
TOOL_GAP = 14.0
TOOLS_PER_FRAME = 3

FLEET_X = 430.0
FLEET_Y = 190.0
FLEET_WIDTH = 1106.0
MACHINE_GAP_X = 18.0
MACHINE_GAP_Y = 14.0
MACHINE_WIDTH = (FLEET_WIDTH - (COLUMNS - 1) * MACHINE_GAP_X) / COLUMNS
MACHINE_HEIGHT = 118.0
FLEET_BOTTOM = FLEET_Y + ROWS * MACHINE_HEIGHT + (ROWS - 1) * MACHINE_GAP_Y

MACHINE_PADDING = 6.0
BAND_GAP = 3.0
INNER_WIDTH = MACHINE_WIDTH - 2 * MACHINE_PADDING
INNER_HEIGHT = MACHINE_HEIGHT - 2 * MACHINE_PADDING
BAND_HEIGHT = (INNER_HEIGHT - 4 * BAND_GAP) / 5.0
LIFECYCLE_WIDTH = 44.0
LIFECYCLE_GAP = 4.0
BAND_WIDTH = INNER_WIDTH - LIFECYCLE_WIDTH - LIFECYCLE_GAP
LIFECYCLE_HEIGHT = 4 * BAND_HEIGHT + 3 * BAND_GAP
BAND_ORDER = (5, 4, 3, 2, 1)

AGENT_MARK = 18.0
AGENT_MARK_OFFSET = 7.0

PORTAINER_MACHINE = 18
PORTAINER_BADGE_Y = FLEET_BOTTOM + 10.0

MACHINE_OUTLINE = Tone(INK_SOFT, WHITE)
PORTAINER_OUTLINE = Tone(BESPOKE.stroke, WHITE)
GOLEM_OUTLINE = Tone(GOLEM.stroke, WHITE)
UNCONFIGURED = Tone(INK_FAINT, WHITE)

EVERY_LAYER = frozenset({1, 2, 3, 4, 5, 6})

LAYER_NAMES: Mapping[int, str] = {
    **{spec.number: spec.title for spec in BAND_LAYERS},
    ORCHESTRATION_LAYER.number: ORCHESTRATION_LAYER.title,
}


class Machine(NamedTuple):
    layers: frozenset[int] = frozenset()
    outline: Tone | None = None
    agent: bool = False


class Tool(NamedTuple):
    name: str
    holds: str
    tone: Tone


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


def every_machine(layers: frozenset[int], **overrides) -> tuple[Machine, ...]:
    return tuple(Machine(layers, **overrides) for _ in range(MACHINE_COUNT))


# NOTE: which box runs Portainer is a fact about the fleet, not about a frame, so it
# outranks whatever outline a frame asked for. Exactly one machine carries it, which
# is the point slide 04 makes in prose and this makes in a colour.
def _outline_for(index: int, machine: Machine) -> Tone:
    if index == PORTAINER_MACHINE:
        return PORTAINER_OUTLINE
    return machine.outline if machine.outline is not None else MACHINE_OUTLINE


def _band(
    scene: Scene, machine: Machine, layer: int, x: float, y: float, width: float, height: float
) -> None:
    configured = layer in machine.layers
    scene.rectangle(
        x,
        y,
        width,
        height,
        DESCRIPTIVE_LAYER_TONES[layer] if configured else UNCONFIGURED,
        stroke_width=1,
        stroke_style="solid" if configured else "dashed",
    )


def _draw_machine(scene: Scene, index: int, machine: Machine) -> dict:
    x, y = machine_origin(index)
    body = scene.rectangle(
        x,
        y,
        MACHINE_WIDTH,
        MACHINE_HEIGHT,
        _outline_for(index, machine),
        stroke_width=2,
    )
    inner_x = x + MACHINE_PADDING
    inner_y = y + MACHINE_PADDING
    for position, layer in enumerate(BAND_ORDER):
        _band(
            scene,
            machine,
            layer,
            inner_x,
            inner_y + position * (BAND_HEIGHT + BAND_GAP),
            INNER_WIDTH if layer == 1 else BAND_WIDTH,
            BAND_HEIGHT,
        )
    _band(
        scene,
        machine,
        ORCHESTRATION_LAYER.number,
        inner_x + BAND_WIDTH + LIFECYCLE_GAP,
        inner_y,
        LIFECYCLE_WIDTH,
        LIFECYCLE_HEIGHT,
    )
    return body


def _draw_key(scene: Scene) -> None:
    for position, layer in enumerate(sorted(LAYER_NAMES)):
        top = KEY_Y + position * KEY_PITCH
        scene.rectangle(
            KEY_X,
            top,
            KEY_SWATCH,
            KEY_SWATCH,
            DESCRIPTIVE_LAYER_TONES[layer],
            label=str(layer),
            label_font_size=CAPTION_SIZE,
        )
        name = wrapped(LAYER_NAMES[layer], KEY_NAME_WIDTH * 0.88, CAPTION_SIZE)
        scene.text(
            KEY_NAME_X,
            top + (KEY_SWATCH - measured_height(name, CAPTION_SIZE)) / 2.0,
            name,
            font_size=CAPTION_SIZE,
            colour=INK_SOFT,
            width=KEY_NAME_WIDTH,
        )


def draw(scene: Scene, machines: Sequence[Machine]) -> Fleet:
    _draw_key(scene)
    bodies = tuple(
        _draw_machine(scene, index, machine) for index, machine in enumerate(machines)
    )
    badge(
        scene,
        machine_centre_x(PORTAINER_MACHINE),
        PORTAINER_BADGE_Y,
        "Portainer",
        tone=PORTAINER_OUTLINE,
        font_size=CAPTION_SIZE,
        anchor="center",
    )
    for index, machine in enumerate(machines):
        if machine.agent:
            x, y = machine_origin(index)
            scene.diamond(
                x - AGENT_MARK_OFFSET,
                y - AGENT_MARK_OFFSET,
                AGENT_MARK,
                AGENT_MARK,
                GOLEM,
                stroke_width=1,
            )
    return Fleet(bodies)


# NOTE: three chips is what fits under the key at BODY_SIZE with a two-line gloss,
# and the type floor forbids buying a fourth by shrinking one. A frame that needs
# more tools than this merges two of them into one chip.
def tool_column(scene: Scene, tools: Sequence[Tool]) -> float:
    if len(tools) > TOOLS_PER_FRAME:
        raise ValueError(
            f"the tool column holds {TOOLS_PER_FRAME} chips, not {len(tools)}"
        )
    for position, tool in enumerate(tools):
        labelled_box(
            scene,
            TOOL_X,
            TOOL_Y + position * (TOOL_HEIGHT + TOOL_GAP),
            TOOL_WIDTH,
            TOOL_HEIGHT,
            LabelledBox(tool.name, tool.holds, tool.tone),
            title_font_size=BODY_SIZE,
            detail_font_size=CAPTION_SIZE,
            padding=12,
        )
    span = len(tools) * TOOL_HEIGHT + (len(tools) - 1) * TOOL_GAP
    return TOOL_Y + span / 2.0


def reaches_the_fleet(scene: Scene, middle_y: float, stroke: str) -> None:
    connector(
        scene,
        [(TOOL_X + TOOL_WIDTH + 10.0, middle_y), (FLEET_X - 10.0, middle_y)],
        stroke=stroke,
        stroke_width=3,
    )
