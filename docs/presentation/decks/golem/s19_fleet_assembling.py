from __future__ import annotations

from excalidraw.layout import slide_header
from excalidraw.palette import ANSIBLE, GOLEM
from excalidraw.scene import Scene

from . import fleet

SLUG = "fleet-assembling"
TITLE = "The fleet: each machine assembles its own scroll"

SUBTITLE = "golemd works through its own difference. Nothing waits on another host."

TOOLS = (
    fleet.Tool("Ansible", "the core OS, network and security", ANSIBLE, work=(1,)),
    fleet.Tool("golemctl", "one manifest, posted to each host", GOLEM),
    fleet.Tool("golemd", "on the host, enacting its own scroll", GOLEM, work=(2, 3, 4, 5, 6)),
)


def build() -> Scene:
    scene = Scene(SLUG, id_namespace=fleet.ID_NAMESPACE)
    slide_header(scene, TITLE, SUBTITLE)
    fleet.draw(scene, fleet.units_arriving(ANSIBLE, GOLEM, 0.5))
    fleet.tool_column(scene, TOOLS)
    return scene
