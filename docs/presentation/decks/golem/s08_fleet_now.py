from __future__ import annotations

from excalidraw.layout import slide_header
from excalidraw.palette import ANSIBLE, BESPOKE
from excalidraw.scene import Scene

from . import fleet

SLUG = "fleet-where-lichess-is-now"
TITLE = "The fleet: where lichess is now"

SUBTITLE = "Ansible keeps 22 units on 8 machines. The other 60 units are still by hand."

TOOLS = (
    fleet.Tool("hosts.py", "which unit runs on which host", BESPOKE),
    fleet.Tool("generated config", "one Ansible file per host", BESPOKE),
    fleet.Tool("Ansible", "the basics, and the units it is given", ANSIBLE, work=(1, 2, 3, 4)),
)


def build() -> Scene:
    scene = Scene(SLUG, id_namespace=fleet.ID_NAMESPACE)
    slide_header(scene, TITLE, SUBTITLE)
    fleet.draw(scene, fleet.units_split(ANSIBLE, ANSIBLE))
    middle = fleet.tool_column(scene, TOOLS)
    fleet.on_one_machine(scene, 2)
    fleet.reaches_the_fleet(scene, middle, ANSIBLE.stroke)
    return scene
