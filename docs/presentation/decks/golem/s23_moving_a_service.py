"""One unit moving between two hosts, drawn at the scale the story happens on.

The fleet-wide view is the wrong frame for a two-machine story: twenty-eight
boxes that do not change swamp the two that do. Both machines are golem's, both
scrolls come out of the same manifest, and the arrow is the unit itself.

The closing note is load-bearing and must keep saying what it says. golem ships
no cross-host ordering: `golemctl fleet` spawns one task per target with no
barrier between them (`apps/golemctl/src/fleet.rs`), and no ADR or TODO proposes
otherwise. What this frame claims over slide 12 is expressibility — three
hand-sequenced edits collapsing to one — and never an orchestrated cutover.
"""

from __future__ import annotations

from excalidraw.layout import badge, connector, note, slide_header, span_bar
from excalidraw.palette import GAP, GOLEM, INK, NEUTRAL, WHITE, Tone
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.text import MONO
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE

from . import fleet

SLUG = "moving-a-service"
TITLE = "Moving a service: one edit, two machines"

SUBTITLE = "The manifest names the host, so both sides fall out of the same apply."

UNIT = "lila-gif"
LOSING = "orbit"
GAINING = "dingo"
LOSING_UNITS = 8
GAINING_UNITS = 3

EDIT_X = MARGIN
EDIT_Y = 236.0
EDIT_WIDTH = 380.0
EDIT_HEIGHT = 168.0
EDIT_PADDING = 22.0

MANIFEST_X = 500.0
MANIFEST_Y = 254.0
MANIFEST_WIDTH = 240.0
MANIFEST_HEIGHT = 132.0

SCROLL_Y = 254.0
SCROLL_WIDTH = 310.0
SCROLL_HEIGHT = 70.0

MACHINE_Y = 400.0
MACHINE_WIDTH = 310.0
MACHINE_HEIGHT = 240.0
LOSING_X = 830.0
GAINING_X = 1180.0

ELBOW_Y = 678.0
BADGE_Y = 716.0
BAR_Y = 786.0
LIMIT_Y = 866.0


def _edit_card(scene: Scene) -> None:
    scene.rectangle(EDIT_X, EDIT_Y, EDIT_WIDTH, EDIT_HEIGHT, NEUTRAL)
    scene.text(
        EDIT_X + EDIT_PADDING,
        EDIT_Y + EDIT_PADDING,
        "one edit",
        font_size=BODY_SIZE,
        colour=INK,
        width=EDIT_WIDTH - 2 * EDIT_PADDING,
    )
    for position, (body, tone) in enumerate(
        ((f"{UNIT} on {LOSING}", GAP), (f"{UNIT} on {GAINING}", GOLEM))
    ):
        scene.text(
            EDIT_X + EDIT_PADDING,
            EDIT_Y + EDIT_PADDING + 46 + position * 44,
            body,
            font_size=CAPTION_SIZE,
            colour=tone.stroke,
            font_family=MONO,
            width=EDIT_WIDTH - 2 * EDIT_PADDING,
        )


def _machine(scene: Scene, x: float, name: str, units: int) -> None:
    fleet.draw_machine(
        scene,
        x,
        MACHINE_Y,
        fleet.Machine(name, tool_units=units, keeper=GOLEM, unit_tone=GOLEM, agent=True),
        width=MACHINE_WIDTH,
        height=MACHINE_HEIGHT,
        name_font_size=BODY_SIZE,
    )


def _unit_cell(x: float, slot: int) -> tuple[float, float, float, float]:
    area = fleet.cell_area(x, MACHINE_Y, MACHINE_WIDTH, MACHINE_HEIGHT)
    return fleet.cell_rect(area, slot)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, TITLE, SUBTITLE)
    _edit_card(scene)
    scene.rectangle(
        MANIFEST_X,
        MANIFEST_Y,
        MANIFEST_WIDTH,
        MANIFEST_HEIGHT,
        GOLEM,
        label="one manifest",
        label_font_size=BODY_SIZE,
    )
    connector(
        scene,
        [(EDIT_X + EDIT_WIDTH + 12, MANIFEST_Y + MANIFEST_HEIGHT / 2.0),
         (MANIFEST_X - 12, MANIFEST_Y + MANIFEST_HEIGHT / 2.0)],
        stroke=GOLEM.stroke,
        stroke_width=3,
    )
    for host, left in ((LOSING, LOSING_X), (GAINING, GAINING_X)):
        fleet.scroll_mark(
            scene, left, SCROLL_Y, SCROLL_WIDTH, SCROLL_HEIGHT, host, GOLEM,
            font_size=BODY_SIZE,
        )
        connector(
            scene,
            [(MANIFEST_X + MANIFEST_WIDTH + 12, MANIFEST_Y + MANIFEST_HEIGHT / 2.0),
             (left - 40, SCROLL_Y - 44),
             (left + SCROLL_WIDTH / 2.0, SCROLL_Y - 12)],
            stroke=GOLEM.stroke,
        )
        connector(
            scene,
            [(left + SCROLL_WIDTH / 2.0, SCROLL_Y + SCROLL_HEIGHT + 10),
             (left + SCROLL_WIDTH / 2.0, MACHINE_Y - 10)],
            stroke=GOLEM.stroke,
            stroke_width=3,
        )
    _machine(scene, LOSING_X, LOSING, LOSING_UNITS - 1)
    _machine(scene, GAINING_X, GAINING, GAINING_UNITS)
    leaving = _unit_cell(LOSING_X, LOSING_UNITS - 1)
    arriving = _unit_cell(GAINING_X, GAINING_UNITS)
    scene.rectangle(
        *leaving, Tone(GAP.stroke, WHITE), radius=False, stroke_width=2, stroke_style="dashed"
    )
    scene.rectangle(*arriving, GOLEM, radius=False, stroke_width=3)
    connector(
        scene,
        [
            (leaving[0] + leaving[2] / 2.0, MACHINE_Y + MACHINE_HEIGHT + 8),
            (leaving[0] + leaving[2] / 2.0, ELBOW_Y),
            (arriving[0] + arriving[2] / 2.0, ELBOW_Y),
            (arriving[0] + arriving[2] / 2.0, MACHINE_Y + MACHINE_HEIGHT + 8),
        ],
        stroke=GOLEM.stroke,
        stroke_width=3,
        label=UNIT,
        font_size=BODY_SIZE,
    )
    badge(
        scene,
        LOSING_X + MACHINE_WIDTH / 2.0,
        BADGE_Y,
        "removes it",
        tone=Tone(GAP.stroke, WHITE, GAP.stroke),
        anchor="center",
    )
    badge(
        scene,
        GAINING_X + MACHINE_WIDTH / 2.0,
        BADGE_Y,
        "installs it",
        tone=Tone(GOLEM.stroke, WHITE, GOLEM.stroke),
        anchor="center",
    )
    span_bar(
        scene,
        MARGIN,
        BAR_Y,
        CONTENT_WIDTH,
        "In golem this is one edit and one apply, and the manifest names the machine.",
        tone=GOLEM,
    )
    note(
        scene,
        MARGIN,
        LIMIT_Y,
        "Nothing orders the two, so both or neither may be running briefly.",
        width=CONTENT_WIDTH,
        colour=GAP.stroke,
    )
    return scene
