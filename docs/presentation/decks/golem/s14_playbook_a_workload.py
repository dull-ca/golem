from __future__ import annotations

from excalidraw.layout import slide_header
from excalidraw.scene import Scene

from . import fleet, playbook

SLUG = "playbook-a-workload"
TITLE = "The playbook, step 4: a workload"

SUBTITLE = "Two hosts, cobar and orbit, appear in two steps."


def build() -> Scene:
    scene = Scene(SLUG, id_namespace=fleet.ID_NAMESPACE)
    slide_header(scene, TITLE, SUBTITLE)
    playbook.draw_frame(scene, 4)
    return scene
