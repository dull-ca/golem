from __future__ import annotations

from excalidraw.layout import slide_header
from excalidraw.palette import ANSIBLE, BESPOKE, MANUAL
from excalidraw.scene import Scene

from . import fleet

SLUG = "fleet-generated-config"
TITLE = "December: the config is generated"

SUBTITLE = "Custom Python runs on one laptop and writes the config Ansible will run."

TOOLS = (
    fleet.Tool("hosts.py", "which unit runs on which host", BESPOKE),
    fleet.Tool("generated config", "one Ansible file per host", BESPOKE),
    fleet.Tool("By hand", "every unit, still", MANUAL, work=(2, 3, 4, 5, 6)),
)


def build() -> Scene:
    scene = Scene(SLUG, id_namespace=fleet.ID_NAMESPACE)
    slide_header(scene, TITLE, SUBTITLE)
    fleet.draw(scene, fleet.units_all_by_hand(ANSIBLE))
    fleet.tool_column(scene, TOOLS)
    fleet.on_one_machine(scene, 2)
    return scene
