from __future__ import annotations

from excalidraw.layout import slide_header
from excalidraw.palette import ANSIBLE, MANUAL
from excalidraw.scene import Scene

from . import lifecycle

SLUG = "today"
TITLE = "How a machine comes to exist today"

SUBTITLE = "Five steps. Ansible does the fourth. A person does the other four."

SPANS = (
    lifecycle.Span(1, 3, "By hand", "the OVH panel, then the installer", MANUAL, True),
    lifecycle.Span(4, 4, "Ansible", "one play", ANSIBLE),
    lifecycle.Span(5, 5, "By hand", "per machine", MANUAL, True),
)


def build() -> Scene:
    scene = Scene(SLUG, id_namespace=lifecycle.ID_NAMESPACE)
    slide_header(scene, TITLE, SUBTITLE)
    lifecycle.draw(scene, SPANS)
    return scene
