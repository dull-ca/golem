from __future__ import annotations

from excalidraw.layout import slide_header
from excalidraw.palette import ANSIBLE, MANUAL
from excalidraw.scene import Scene

from . import fleet

SLUG = "fleet-by-hand"
TITLE = "The fleet: the rest by hand"

SUBTITLE = "Layers 2 to 6 configured machine by machine. Coverage differs between machines."

TOOLS = (
    fleet.Tool("Ansible", "layer 1, on every machine", ANSIBLE),
    fleet.Tool("By hand", "layers 2 to 6, one machine at a time", MANUAL),
)

# NOTE: the ragged coverage is the content of this frame, not decoration — it is what
# separates it from the next one, where generated config makes every machine alike.
# Written out rather than derived, so a reviewer can see exactly what is claimed.
BY_HAND = (
    (1, 2, 3, 4, 5, 6),
    (1, 2, 4, 5, 6),
    (1, 2, 3, 5, 6),
    (1, 2, 4),
    (1, 2, 3, 4, 5, 6),
    (1, 2, 5, 6),
    (1, 2, 3, 4),
    (1, 2, 3, 4, 5, 6),
    (1, 2, 4, 5, 6),
    (1, 2),
    (1, 2, 3, 4, 5, 6),
    (1, 2, 3, 5, 6),
    (1, 2, 4, 5, 6),
    (1, 2, 3, 4, 5, 6),
    (1, 2, 3),
    (1, 2, 4, 5, 6),
    (1, 2, 3, 4, 5, 6),
    (1, 2, 5, 6),
    (1, 2, 4, 5, 6),
    (1, 2, 3, 4, 5, 6),
    (1, 2, 3, 4),
    (1, 2, 4, 5, 6),
    (1, 2, 3, 5, 6),
    (1, 2, 3, 4, 5, 6),
)


def build() -> Scene:
    scene = Scene(SLUG, id_namespace=fleet.ID_NAMESPACE)
    slide_header(scene, TITLE, SUBTITLE)
    fleet.draw(
        scene, tuple(fleet.Machine(frozenset(layers)) for layers in BY_HAND)
    )
    middle = fleet.tool_column(scene, TOOLS)
    fleet.reaches_the_fleet(scene, middle, ANSIBLE.stroke)
    return scene
