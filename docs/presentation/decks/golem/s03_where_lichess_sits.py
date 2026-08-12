from __future__ import annotations

from excalidraw.layout import (
    Tick,
    badge,
    connector,
    note,
    slide_header,
    span_bar,
    split_compare,
    timeline,
)
from excalidraw.palette import BESPOKE, INK_FAINT, PLATFORM, YOURS
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene, bottom_edge
from excalidraw.type_scale import BODY_SIZE

SLUG = "where-lichess-sits"
TITLE = "Where lichess sits"

ANNOTATION_Y = 200.0
ANNOTATION_HEIGHT = 56.0
TIMELINE_Y = 300.0
MARKER_HEIGHT = 44.0
COMPARISON_Y = 560.0
COMPARISON_HEIGHT = 190.0

TICKS = (
    Tick("Bare metal + config mgmt", tone=YOURS),
    Tick("Docker (one host)", tone=YOURS),
    Tick("Swarm", tone=PLATFORM),
    Tick("Nomad", tone=PLATFORM),
    Tick("Kubernetes", tone=PLATFORM),
    Tick("Managed Kubernetes", tone=PLATFORM),
)

PORTAINER_FIRST = 1
PORTAINER_LAST = 4


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, "Where lichess sits")
    step = CONTENT_WIDTH / len(TICKS)
    lichess_x = MARGIN + step / 2.0
    marker = badge(
        scene,
        lichess_x,
        ANNOTATION_Y,
        "lichess is here",
        tone=YOURS,
        font_size=BODY_SIZE,
        anchor="center",
        height=ANNOTATION_HEIGHT,
    )
    portainer_left = MARGIN + PORTAINER_FIRST * step + 10
    portainer_right = MARGIN + (PORTAINER_LAST + 1) * step - 10
    span_bar(
        scene,
        portainer_left,
        ANNOTATION_Y,
        portainer_right - portainer_left,
        "Portainer — a web UI that manages these platforms",
        tone=BESPOKE,
        height=ANNOTATION_HEIGHT,
    )
    axis_y = TIMELINE_Y + MARKER_HEIGHT
    connector(
        scene,
        [(lichess_x, bottom_edge(marker) + 6), (lichess_x, axis_y - 18)],
        stroke=INK_FAINT,
        dashed=True,
    )
    timeline(
        scene,
        MARGIN,
        TIMELINE_Y,
        CONTENT_WIDTH,
        TICKS,
        marker_height=MARKER_HEIGHT,
    )
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
