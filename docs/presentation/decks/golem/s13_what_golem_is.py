from __future__ import annotations

from excalidraw.layout import note, slide_header, span_bar, split_compare
from excalidraw.palette import GOLEM, MANUAL
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene

SLUG = "what-golem-is"
TITLE = "What golem is, and is not"

PANELS_Y = 200.0
PANELS_HEIGHT = 440.0
BAR_Y = 680.0
BAR_HEIGHT = 58.0
CLOSING_Y = 774.0

NOT_BODY = (
    "a replacement for bare-metal provisioning, OS installation, or the basics of "
    "networking and security"
)

IS_BODY = (
    "a replacement for the custom Python and the new Ansible we were building in "
    "December and January"
)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, "What golem is, and is not")
    not_panel, is_panel = split_compare(
        scene,
        MARGIN,
        PANELS_Y,
        CONTENT_WIDTH,
        PANELS_HEIGHT,
        ("Not", MANUAL),
        ("Is", GOLEM),
    )
    note(
        scene,
        not_panel.body.x,
        not_panel.body.y,
        NOT_BODY,
        width=not_panel.body.width,
    )
    note(
        scene,
        is_panel.body.x,
        is_panel.body.y,
        IS_BODY,
        width=is_panel.body.width,
    )
    span_bar(
        scene,
        MARGIN,
        BAR_Y,
        CONTENT_WIDTH,
        "Layer 1 stays where it is.",
        tone=GOLEM,
        height=BAR_HEIGHT,
    )
    note(
        scene,
        MARGIN,
        CLOSING_Y,
        "You write the state a host should be in. golemd works out the steps.",
        width=CONTENT_WIDTH,
    )
    return scene
