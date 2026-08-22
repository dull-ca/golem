from __future__ import annotations

from excalidraw.layout import note, slide_header, split_compare
from excalidraw.palette import PLATFORM, YOURS
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene

from . import lichess_ladder

SLUG = "where-lichess-sits"
TITLE = "Where lichess sits"

COMPARISON_Y = 560.0
COMPARISON_HEIGHT = 190.0


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, TITLE)
    lichess_ladder.draw(scene)
    left, right = split_compare(
        scene,
        MARGIN,
        COMPARISON_Y,
        CONTENT_WIDTH,
        COMPARISON_HEIGHT,
        ("You name the machine", YOURS),
        ("The platform picks the machine", PLATFORM),
    )
    note(
        scene,
        left.body.x,
        left.body.y,
        "Bare metal with configuration management, and Docker on one host.",
        width=left.body.width,
    )
    note(
        scene,
        right.body.x,
        right.body.y,
        "Swarm, Nomad, Kubernetes, and managed Kubernetes.",
        width=right.body.width,
    )
    return scene
