from __future__ import annotations

from excalidraw.layout import slide_header
from excalidraw.palette import ANSIBLE, MANUAL
from excalidraw.scene import Scene

from . import fleet

SLUG = "fleet-by-hand"
TITLE = "The fleet: the rest by hand"

SUBTITLE = "Eighty-two units, placed one machine at a time. No two machines alike."

TOOLS = (
    fleet.Tool("Ansible", "the core OS, network and security", ANSIBLE, work=(1,)),
    fleet.Tool("By hand", "every unit, one machine at a time", MANUAL, work=(2, 3, 4, 5, 6)),
)


def build() -> Scene:
    scene = Scene(SLUG, id_namespace=fleet.ID_NAMESPACE)
    slide_header(scene, TITLE, SUBTITLE)
    fleet.draw(scene, fleet.units_all_by_hand(ANSIBLE))
    middle = fleet.tool_column(scene, TOOLS)
    fleet.reaches_the_fleet(scene, middle, ANSIBLE.stroke)
    return scene
