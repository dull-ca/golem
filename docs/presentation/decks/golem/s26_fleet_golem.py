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

from ..lichess_fleet import HAND_KEPT_HOST_COUNT, HOST_COUNT, TOOL_KEPT_HOSTS
from . import fleet, golem_symbol

SLUG = "fleet-golem"

MACHINES_KEPT = len(TOOL_KEPT_HOSTS)
# NOTE: the count stays in the title. Without one the title implies all thirty,
# which is what s27 draws as the fleet we want, and the two frames carry the same
# geometry -- they must not be confusable.
TITLE = f"The fleet: golem keeps units on {MACHINES_KEPT} of {HOST_COUNT} machines"

SUBTITLE = (
    f"The same {MACHINES_KEPT} machines, five kinds of work instead of three. "
    f"{HAND_KEPT_HOST_COUNT} are still by hand."
)

SYMBOL_X = 1440.0
SYMBOL_Y = 826.0
SYMBOL_HEIGHT = 96.0
CREDIT_X = 1016.0
CREDIT_Y = 874.0
CREDIT_WIDTH = 404.0

TOOLS = (
    fleet.Tool("Ansible", "the core OS, network and security", ANSIBLE, work=(1,)),
    fleet.Tool("emetc, golemctl", "compile, then submit the manifest", GOLEM),
    fleet.Tool("golemd", "on the host, keeping its own units", GOLEM, work=(2, 3, 4, 5, 6)),
)


def build() -> Scene:
    scene = Scene(SLUG, id_namespace=fleet.ID_NAMESPACE)
    fleet.check_header(slide_header(scene, TITLE, SUBTITLE))
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
