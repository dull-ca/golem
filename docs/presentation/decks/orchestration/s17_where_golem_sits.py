from __future__ import annotations

from excalidraw.layout import note, slide_header, span_bar, split_compare
from excalidraw.palette import GAP, GOLEM
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import BODY_SIZE

SLUG = "where-golem-sits"
TITLE = "Where golem sits"

PANELS_Y = 200.0
PANELS_HEIGHT = 420.0
BAR_Y = 660.0
BAR_HEIGHT = 64.0
CLOSING_Y = 762.0

IS_BODY = (
    "declarative desired state, and reversible enactment: every edit records its "
    "inverse, so a change can be taken back exactly"
)

IS_NOT_BODY = (
    "a scheduler. Nothing in golem answers which node. The program names the host, "
    "the same way a person would"
)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        "Where golem sits",
        "One of the five jobs is deliberately left out.",
    )
    is_panel, is_not_panel = split_compare(
        scene,
        MARGIN,
        PANELS_Y,
        CONTENT_WIDTH,
        PANELS_HEIGHT,
        ("golem is", GOLEM),
        ("golem is not", GAP),
    )
    note(
        scene,
        is_panel.body.x,
        is_panel.body.y,
        IS_BODY,
        width=is_panel.body.width,
        font_size=BODY_SIZE,
    )
    note(
        scene,
        is_not_panel.body.x,
        is_not_panel.body.y,
        IS_NOT_BODY,
        width=is_not_panel.body.width,
        font_size=BODY_SIZE,
    )
    span_bar(
        scene,
        MARGIN,
        BAR_Y,
        CONTENT_WIDTH,
        "That boundary is the point, not an omission.",
        tone=GOLEM,
        height=BAR_HEIGHT,
    )
    note(
        scene,
        MARGIN,
        CLOSING_Y,
        "Placement stays a decision a person makes, written down and versioned.",
        width=CONTENT_WIDTH,
    )
    return scene
