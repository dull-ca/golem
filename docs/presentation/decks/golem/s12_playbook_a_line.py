from __future__ import annotations

from excalidraw.layout import slide_header
from excalidraw.scene import Scene

from . import fleet, playbook

SLUG = "playbook-a-line"
TITLE = "The playbook, step 2: a line in a file"


def build() -> Scene:
    scene = Scene(SLUG, id_namespace=fleet.ID_NAMESPACE)
    slide_header(scene, TITLE)
    playbook.draw_frame(scene, 2)
    return scene
