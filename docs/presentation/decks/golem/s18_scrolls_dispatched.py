from __future__ import annotations

from excalidraw.layout import connector, note, slide_header
from excalidraw.palette import ANSIBLE, GOLEM, INK_FAINT
from excalidraw.scene import MARGIN, Scene
from excalidraw.text import MONO
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE

from . import fleet
from ..lichess_fleet import HOSTS

SLUG = "golem-scrolls-dispatched"
TITLE = "golem: each scroll goes to the machine it names"

SUBTITLE = "golemctl addresses one host at a time. There is no broadcast and no controller."

SENDER_X = MARGIN
SENDER_Y = 430.0
SENDER_WIDTH = 300.0
SENDER_HEIGHT = 110.0

SCROLL_X = 430.0
SCROLL_WIDTH = 250.0
SCROLL_HEIGHT = 70.0

MACHINE_X = 760.0
MACHINE_WIDTH = 237.0
MACHINE_HEIGHT = 160.0

ROW_Y = 200.0
ROW_PITCH = 182.0

NOTE_X = 1060.0
NOTE_Y = 380.0
NOTE_WIDTH = 476.0

ROUTED = ("orbit", "cobar", "dingo", "achoo")


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, TITLE, SUBTITLE)
    scene.rectangle(
        SENDER_X,
        SENDER_Y,
        SENDER_WIDTH,
        SENDER_HEIGHT,
        GOLEM,
        label="golemctl fleet apply",
        label_font_size=BODY_SIZE,
        label_font_family=MONO,
    )
    for position, name in enumerate(ROUTED):
        host = next(entry for entry in HOSTS if entry.name == name)
        top = ROW_Y + position * ROW_PITCH
        scroll_top = top + (MACHINE_HEIGHT - SCROLL_HEIGHT) / 2.0
        fleet.scroll_mark(
            scene, SCROLL_X, scroll_top, SCROLL_WIDTH, SCROLL_HEIGHT, name, GOLEM
        )
        connector(
            scene,
            [(SENDER_X + SENDER_WIDTH + 10, SENDER_Y + SENDER_HEIGHT / 2.0),
             (SCROLL_X - 10, scroll_top + SCROLL_HEIGHT / 2.0)],
            stroke=GOLEM.stroke,
        )
        connector(
            scene,
            [(SCROLL_X + SCROLL_WIDTH + 10, scroll_top + SCROLL_HEIGHT / 2.0),
             (MACHINE_X - 10, top + MACHINE_HEIGHT / 2.0)],
            stroke=GOLEM.stroke,
            stroke_width=3,
        )
        fleet.draw_machine(
            scene,
            MACHINE_X,
            top,
            fleet.Machine(
                name,
                tool_units=host.tool_units,
                keeper=ANSIBLE,
                unit_tone=GOLEM,
                agent=True,
            ),
            width=MACHINE_WIDTH,
            height=MACHINE_HEIGHT,
            name_font_size=BODY_SIZE,
        )
    note(
        scene,
        NOTE_X,
        NOTE_Y,
        "golemd on the host takes its own scroll, diffs it against what it "
        "last applied, and enacts the difference.",
        width=NOTE_WIDTH,
    )
    note(
        scene,
        NOTE_X,
        NOTE_Y + 140,
        "Four of thirty shown.",
        width=NOTE_WIDTH,
        font_size=CAPTION_SIZE,
        colour=INK_FAINT,
    )
    return scene
