from __future__ import annotations

from excalidraw.layout import LabelledBox, connector, labelled_box, note, slide_header
from excalidraw.palette import ANSIBLE
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE

from .. import machines

SLUG = "ansible-pushes"
TITLE = "Ansible: one controller, every machine"

SUBTITLE = "One run opens a connection to every host named in the inventory."

CONTROLLER_X = MARGIN + 14.0
CONTROLLER_Y = 402.0
CONTROLLER_WIDTH = 336.0
CONTROLLER_HEIGHT = 164.0
BUS_X = machines.FLEET_X - 26.0
CLOSING_Y = 862.0


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, TITLE, SUBTITLE)
    machines.draw_fleet(scene, machines.baseline_machines(ANSIBLE))
    labelled_box(
        scene,
        CONTROLLER_X,
        CONTROLLER_Y,
        CONTROLLER_WIDTH,
        CONTROLLER_HEIGHT,
        LabelledBox(
            "Ansible controller",
            "one machine, holding the playbooks and the inventory",
            ANSIBLE,
        ),
        title_font_size=BODY_SIZE,
        detail_font_size=CAPTION_SIZE,
    )
    scene.line(
        [(BUS_X, machines.FLEET_Y), (BUS_X, machines.FLEET_BOTTOM)],
        stroke=ANSIBLE.stroke,
        stroke_width=3,
    )
    connector(
        scene,
        [
            (CONTROLLER_X + CONTROLLER_WIDTH + 10.0, CONTROLLER_Y + CONTROLLER_HEIGHT / 2.0),
            (BUS_X, CONTROLLER_Y + CONTROLLER_HEIGHT / 2.0),
        ],
        stroke=ANSIBLE.stroke,
        stroke_width=3,
        arrowhead=False,
    )
    for row in range(machines.ROWS):
        middle = (
            machines.FLEET_Y
            + row * (machines.MACHINE_HEIGHT + machines.MACHINE_GAP_Y)
            + machines.MACHINE_HEIGHT / 2.0
        )
        connector(
            scene,
            [(BUS_X, middle), (machines.FLEET_X - 6.0, middle)],
            stroke=ANSIBLE.stroke,
            stroke_width=3,
        )
    note(
        scene,
        MARGIN,
        CLOSING_Y,
        "Nothing runs on a host between runs. The controller is the only thing that acts.",
        width=CONTENT_WIDTH,
    )
    return scene
