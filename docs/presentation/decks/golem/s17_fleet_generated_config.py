from __future__ import annotations

from excalidraw.layout import note, slide_header
from excalidraw.palette import ANSIBLE, BESPOKE
from excalidraw.scene import Scene

from . import fleet

SLUG = "fleet-generated-config"
TITLE = "The fleet: where lichess is now"

SUBTITLE = "Custom Python generates the config Ansible runs, and Ansible installs the services."

NOTE_Y = 800.0

TOOLS = (
    fleet.Tool("Custom Python", "generates the config Ansible runs", BESPOKE),
    fleet.Tool("Ansible", "layer 1, and installs the services", ANSIBLE),
)


def build() -> Scene:
    scene = Scene(SLUG, id_namespace=fleet.ID_NAMESPACE)
    slide_header(scene, TITLE, SUBTITLE)
    fleet.draw(scene, fleet.every_machine(fleet.EVERY_LAYER))
    middle = fleet.tool_column(scene, TOOLS)
    fleet.reaches_the_fleet(scene, middle, ANSIBLE.stroke)
    note(
        scene,
        fleet.FLEET_X,
        NOTE_Y,
        "Generated config is what makes the fleet uniform: every machine gets the same layers.",
        width=fleet.FLEET_WIDTH,
    )
    return scene
