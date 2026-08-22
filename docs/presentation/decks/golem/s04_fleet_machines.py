from __future__ import annotations

from excalidraw.layout import slide_header
from excalidraw.scene import Scene

from . import fleet

SLUG = "fleet-machines"
TITLE = "The fleet: thirty machines"

SUBTITLE = "Rented bare metal, named as the inventory names them. A box holds its units."


def build() -> Scene:
    scene = Scene(SLUG, id_namespace=fleet.ID_NAMESPACE)
    slide_header(scene, TITLE, SUBTITLE)
    fleet.draw(scene, fleet.bare_machines())
    return scene
