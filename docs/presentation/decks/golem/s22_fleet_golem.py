"""The frame where golem has finished, and the one that refuses to overstate it.

No arrow runs from the tool column into the fleet: a playbook and a generator act
on the fleet from outside it, and golem does not. Each machine carries its own
mark instead, because golemd on the host is what enacts the scroll.

The imported mark appears once, at full size, and is credited on the slide because
CC BY 3.0 requires it — see `assets/README.md`.
"""

from __future__ import annotations

from excalidraw.layout import note, slide_header
from excalidraw.palette import ANSIBLE, GOLEM, INK_FAINT
from excalidraw.scene import Scene
from excalidraw.type_scale import CAPTION_SIZE

from . import fleet, golem_symbol

SLUG = "fleet-golem"
TITLE = "The fleet: what golem keeps"

SUBTITLE = "The same 8 machines, five kinds of work instead of three. 22 are still by hand."

SYMBOL_X = 1180.0
SYMBOL_Y = 826.0
SYMBOL_HEIGHT = 96.0
CREDIT_X = 1300.0
CREDIT_Y = 852.0
CREDIT_WIDTH = 236.0

TOOLS = (
    fleet.Tool("Ansible", "the core OS, network and security", ANSIBLE, work=(1,)),
    fleet.Tool("emetc, golemctl", "compile, then submit the manifest", GOLEM),
    fleet.Tool("golemd", "on the host, keeping its own units", GOLEM, work=(2, 3, 4, 5, 6)),
)


def build() -> Scene:
    scene = Scene(SLUG, id_namespace=fleet.ID_NAMESPACE)
    slide_header(scene, TITLE, SUBTITLE)
    fleet.draw(scene, fleet.units_split(ANSIBLE, GOLEM, agents=True))
    fleet.tool_column(scene, TOOLS)
    scene.image(SYMBOL_X, SYMBOL_Y, SYMBOL_HEIGHT, golem_symbol.mark())
    note(
        scene,
        CREDIT_X,
        CREDIT_Y,
        golem_symbol.CREDIT,
        width=CREDIT_WIDTH,
        font_size=CAPTION_SIZE,
        colour=INK_FAINT,
    )
    return scene
