from __future__ import annotations

from excalidraw.layout import slide_header
from excalidraw.palette import ANSIBLE
from excalidraw.scene import Scene

from . import fleet

SLUG = "fleet-basics"
TITLE = "The fleet: Ansible does the basics"

SUBTITLE = "Core OS, network and security on all thirty. Nothing runs on them yet."

TOOLS = (fleet.Tool("Ansible", "the core OS, network and security", ANSIBLE, work=(1,)),)


def build() -> Scene:
    scene = Scene(SLUG, id_namespace=fleet.ID_NAMESPACE)
    slide_header(scene, TITLE, SUBTITLE)
    fleet.draw(scene, fleet.baseline_machines(ANSIBLE))
    middle = fleet.tool_column(scene, TOOLS)
    fleet.reaches_the_fleet(scene, middle, ANSIBLE.stroke)
    return scene
