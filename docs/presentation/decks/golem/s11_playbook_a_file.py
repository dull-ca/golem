from __future__ import annotations

from excalidraw.layout import slide_header
from excalidraw.scene import Scene

from . import fleet, playbook

SLUG = "playbook-a-file"
TITLE = "The playbook, step 1: a file"

SUBTITLE = "Each step in this play names a group of hosts rather than the whole fleet."


def build() -> Scene:
    scene = Scene(SLUG, id_namespace=fleet.ID_NAMESPACE)
    slide_header(scene, TITLE, SUBTITLE)
    playbook.draw_frame(scene, 1)
    return scene
