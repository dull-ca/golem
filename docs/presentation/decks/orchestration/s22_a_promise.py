from __future__ import annotations

from excalidraw.layout import LabelledBox, connector, labelled_box, note, slide_header
from excalidraw.palette import GAP, GOLEM, INK_FAINT
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE

from .. import machines

SLUG = "a-promise"
TITLE = "A promise is about your own state"

CHIP_X = MARGIN + 14.0
CHIP_Y = 304.0
CHIP_WIDTH = 336.0
CHIP_HEIGHT = 168.0
CHIP_GAP = 24.0

MACHINE_X = 470.0
MACHINE_WIDTH = 300.0
MACHINE_PITCH = 383.0
MACHINE_Y = 354.0
MACHINE_HEIGHT = 220.0
SCROLL_Y = 236.0
SCROLL_WIDTH = 150.0
SCROLL_HEIGHT = 62.0
LOOP_BOTTOM = 616.0

CLOSING_Y = 706.0

HOSTS = ("achoo", "cobar", "orbit")
UNITS = (2, 5, 8)

CHIPS = (
    LabelledBox(
        "The state it should be in",
        "written down and versioned, one description for the whole fleet",
        GOLEM,
    ),
    LabelledBox(
        "An agent on the host",
        "works out its own steps, and records how to undo each one",
        GOLEM,
    ),
)


def _machine_x(position: int) -> float:
    return MACHINE_X + position * MACHINE_PITCH


def _draw_self_loop(scene: Scene, left: float) -> None:
    connector(
        scene,
        [
            (left + MACHINE_WIDTH - 40.0, MACHINE_Y + MACHINE_HEIGHT + 6.0),
            (left + MACHINE_WIDTH - 40.0, LOOP_BOTTOM),
            (left + 40.0, LOOP_BOTTOM),
            (left + 40.0, MACHINE_Y + MACHINE_HEIGHT + 6.0),
        ],
        stroke=GOLEM.stroke,
        stroke_width=2,
    )


def _draw_barrier(scene: Scene, left: float) -> None:
    middle = MACHINE_Y + MACHINE_HEIGHT / 2.0
    span = MACHINE_PITCH - MACHINE_WIDTH
    start = left + MACHINE_WIDTH + 10.0
    end = start + span - 20.0
    scene.line(
        [(start, middle), (end, middle)],
        stroke=INK_FAINT,
        stroke_width=2,
        stroke_style="dashed",
    )
    centre_x = (start + end) / 2.0
    for direction in (1.0, -1.0):
        scene.line(
            [
                (centre_x - 16.0, middle - 16.0 * direction),
                (centre_x + 16.0, middle + 16.0 * direction),
            ],
            stroke=GAP.stroke,
            stroke_width=3,
        )


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, TITLE)
    for position, chip in enumerate(CHIPS):
        labelled_box(
            scene,
            CHIP_X,
            CHIP_Y + position * (CHIP_HEIGHT + CHIP_GAP),
            CHIP_WIDTH,
            CHIP_HEIGHT,
            chip,
            title_font_size=BODY_SIZE,
            detail_font_size=CAPTION_SIZE,
        )
    for position, (host, units) in enumerate(zip(HOSTS, UNITS)):
        left = _machine_x(position)
        machines.scroll_mark(
            scene,
            left + (MACHINE_WIDTH - SCROLL_WIDTH) / 2.0,
            SCROLL_Y,
            SCROLL_WIDTH,
            SCROLL_HEIGHT,
            host,
            GOLEM,
        )
        connector(
            scene,
            [
                (left + MACHINE_WIDTH / 2.0, SCROLL_Y + SCROLL_HEIGHT + 6.0),
                (left + MACHINE_WIDTH / 2.0, MACHINE_Y - 6.0),
            ],
            stroke=GOLEM.stroke,
            stroke_width=2,
        )
        machines.draw_machine(
            scene,
            left,
            MACHINE_Y,
            machines.Machine(
                host,
                tool_units=units,
                keeper=GOLEM,
                unit_tone=GOLEM,
                agent=True,
            ),
            width=MACHINE_WIDTH,
            height=MACHINE_HEIGHT,
            name_font_size=BODY_SIZE,
        )
        _draw_self_loop(scene, left)
        if position < len(HOSTS) - 1:
            _draw_barrier(scene, left)
    note(
        scene,
        MACHINE_X,
        LOOP_BOTTOM + 16.0,
        "Each machine acts on itself, and on nothing else.",
        width=CONTENT_WIDTH - (MACHINE_X - MARGIN),
        font_size=CAPTION_SIZE,
        align="center",
    )
    note(
        scene,
        MARGIN,
        CLOSING_Y,
        "Nothing orders one machine against the next, so a service moving from one "
        "host to another can be on both, or on neither, for a moment.",
        width=CONTENT_WIDTH,
    )
    return scene
