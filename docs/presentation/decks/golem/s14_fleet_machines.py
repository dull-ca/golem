from __future__ import annotations

from excalidraw.layout import slide_header
from excalidraw.scene import Scene

from . import fleet

SLUG = "fleet-machines"
TITLE = "The fleet: twenty-four machines"

SUBTITLE = "Rented bare metal, nothing configured. One of them runs Portainer."


def build() -> Scene:
    scene = Scene(SLUG, id_namespace=fleet.ID_NAMESPACE)
    slide_header(scene, TITLE, SUBTITLE)
    fleet.draw(scene, fleet.every_machine(frozenset()))
    return scene
