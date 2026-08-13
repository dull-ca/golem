from __future__ import annotations

from excalidraw.layout import note, slide_header
from excalidraw.palette import ANSIBLE, MANUAL, PLATFORM, THEIRS
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene

from . import stack

SLUG = "which-product"
TITLE = "Which product answers which part"

SUBTITLE = "Several of them overlap, and none of them covers the stack."

CLOSING_Y = 878.0


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, TITLE, SUBTITLE)
    stack.draw(scene, column_tone=PLATFORM, column_tag="Kubernetes or Nomad")
    stack.gutter_bar(
        scene, (0, 0), (1, 3), "OVH", THEIRS, detail="rented, racked, on the network"
    )
    stack.gutter_bar(
        scene, (0, 0), (4, 5), "Ansible", ANSIBLE, detail="the basic setup and configuration"
    )
    stack.gutter_bar(
        scene,
        (1, 1),
        (4, 4),
        "By hand",
        MANUAL,
        stroke_style="dashed",
    )
    stack.gutter_bar(scene, (1, 1), (5, 5), "Kubernetes", PLATFORM)
    stack.gutter_bar(scene, (2, 2), (5, 5), "Nomad", PLATFORM)
    stack.enclose(scene, (1, 2), (5, 5), "Portainer — a web UI over these two")
    note(
        scene,
        MARGIN,
        CLOSING_Y,
        "The image supplies the runtimes and the applications themselves.",
        width=CONTENT_WIDTH,
    )
    return scene
