from __future__ import annotations

from excalidraw.layout import slide_header
from excalidraw.palette import ANSIBLE
from excalidraw.scene import Scene

from . import fleet

SLUG = "fleet-basics"
TITLE = "The fleet: Ansible does the basics"

SUBTITLE = "Security, the private network and the core OS — the same layer 1 everywhere."

TOOLS = (fleet.Tool("Ansible", "layer 1, on every machine", ANSIBLE),)


def build() -> Scene:
    scene = Scene(SLUG, id_namespace=fleet.ID_NAMESPACE)
    slide_header(scene, TITLE, SUBTITLE)
    fleet.draw(scene, fleet.every_machine(frozenset({1})))
    middle = fleet.tool_column(scene, TOOLS)
    fleet.reaches_the_fleet(scene, middle, ANSIBLE.stroke)
    return scene
