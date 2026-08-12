"""The frame where golem enters, and the only one that draws no arrow at the fleet.

Frames 15 to 17 run a connector from the tool column into the machines, because a
playbook and a generator do act on the fleet from outside it. Here that arrow is
absent and every machine carries its own mark instead: golemctl posts the manifest
and each golemd diffs its own scroll. The absence is the argument, so restoring the
connector here would draw a central controller that golem does not have.

The imported mark appears once, at full size, and is credited on the slide because
CC BY 3.0 requires it — see `assets/README.md`. The per-machine agents stay drawn
marks: a filled 512-unit silhouette repeated at 18px is a blob, and it would fight
the layer colours it sits beside.
"""

from __future__ import annotations

from excalidraw.layout import note, slide_header
from excalidraw.palette import ANSIBLE, GOLEM, INK_FAINT
from excalidraw.scene import Scene
from excalidraw.type_scale import CAPTION_SIZE

from . import fleet, golem_symbol

SLUG = "fleet-golem"
TITLE = "The fleet: golem on every machine"

SUBTITLE = "Ansible keeps layer 1 and installs golemd. Each machine then configures itself."

SYMBOL_X = 620.0
SYMBOL_Y = 760.0
SYMBOL_HEIGHT = 110.0
CREDIT_X = 754.0
CREDIT_Y = 800.0
CREDIT_WIDTH = 400.0
NOTE_Y = 886.0

TOOLS = (
    fleet.Tool("Ansible", "layer 1, and installs golemd", ANSIBLE),
    fleet.Tool("emetc, golemctl", "compile the program, submit the manifest", GOLEM),
    fleet.Tool("golemd", "on every machine — layers 2 to 6", GOLEM),
)


def build() -> Scene:
    scene = Scene(SLUG, id_namespace=fleet.ID_NAMESPACE)
    slide_header(scene, TITLE, SUBTITLE)
    fleet.draw(
        scene,
        fleet.every_machine(
            fleet.EVERY_LAYER, outline=fleet.GOLEM_OUTLINE, agent=True
        ),
    )
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
    note(
        scene,
        fleet.FLEET_X,
        NOTE_Y,
        "golemctl posts the manifest; each golemd diffs its own scroll and enacts it.",
        width=fleet.FLEET_WIDTH,
    )
    return scene
