from __future__ import annotations

from excalidraw.layout import slide_header
from excalidraw.palette import ANSIBLE, GOLEM, PULUMI
from excalidraw.scene import Scene

from . import lifecycle

SLUG = "the-proposal"
TITLE = "The proposal"

SUBTITLE = "Pulumi takes the first three steps. Ansible keeps the fourth. golem takes the fifth."

SPANS = (
    lifecycle.Span(1, 3, "Pulumi", "one resource at the provider", PULUMI),
    lifecycle.Span(4, 4, "Ansible", "unchanged", ANSIBLE),
    lifecycle.Span(5, 5, "golem", "one scroll per host", GOLEM),
)


def build() -> Scene:
    scene = Scene(SLUG, id_namespace=lifecycle.ID_NAMESPACE)
    slide_header(scene, TITLE, SUBTITLE)
    lifecycle.draw(scene, SPANS)
    return scene
