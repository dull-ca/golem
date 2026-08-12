"""Slide 03's figure recoloured: buying orchestration answers all of layer 6 at once.

The point is the contrast with 06 and 07, which recolour the identical shape. Here
one platform claims layers 2, 3 and 6 and every orchestration part inside 6; the
container image carries 4 and 5; layer 1 is the remainder that stays ours.
"""

from __future__ import annotations

from excalidraw.layout import callout, connector, legend, note, slide_header
from excalidraw.palette import BLUE, CONTAINER, HOSTED, PLATFORM, YOURS
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene, bottom_edge, centre

from . import lichess_stack

SLUG = "bought-orchestration"
TITLE = "If we bought orchestration"

FIGURE_Y = 196
FIGURE_HEIGHT = 520

PLATFORM_TAG = "Nomad / K8s"
IMAGE_TAG = "OCI image"

LAYER_TONES = {
    1: YOURS,
    2: PLATFORM,
    3: PLATFORM,
    4: CONTAINER,
    5: CONTAINER,
    6: PLATFORM,
}

LAYER_TAGS = {
    1: "still ours",
    2: PLATFORM_TAG,
    3: PLATFORM_TAG,
    4: IMAGE_TAG,
    5: IMAGE_TAG,
    6: PLATFORM_TAG,
}

PART_TONES = {part.number: PLATFORM for part in lichess_stack.ORCHESTRATION_PARTS}

PART_TAGS = {part.number: "included" for part in lichess_stack.ORCHESTRATION_PARTS}

MANAGED_CALLOUT_WIDTH = 900
MANAGED_CALLOUT_X = MARGIN + (CONTENT_WIDTH - MANAGED_CALLOUT_WIDTH) / 2.0


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        "If we bought orchestration",
        "Nomad or Kubernetes: the same six layers, coloured by who answers them.",
    )
    figure = lichess_stack.draw(
        scene,
        y=FIGURE_Y,
        height=FIGURE_HEIGHT,
        layer_tones=LAYER_TONES,
        layer_tags=LAYER_TAGS,
        part_tones=PART_TONES,
        part_tags=PART_TAGS,
        show_details=False,
    )
    ground = figure.layer(1)
    banner = callout(
        scene,
        MANAGED_CALLOUT_X,
        figure.bottom + 24,
        MANAGED_CALLOUT_WIDTH,
        "Renting managed Kubernetes covers layer 1 too — and costs more.",
        tone=HOSTED,
    )
    connector(
        scene,
        [
            (centre(banner)[0], banner["y"] - 4),
            (centre(banner)[0], bottom_edge(ground) + 4),
        ],
        stroke=BLUE,
        dashed=True,
    )
    legend(
        scene,
        MARGIN,
        bottom_edge(banner) + 22,
        (
            (PLATFORM, "the platform answers it"),
            (CONTAINER, "the container image carries it"),
            (YOURS, "still ours to build and keep"),
        ),
    )
    note(
        scene,
        MARGIN,
        bottom_edge(banner) + 62,
        "All five parts of layer 6 arrive together — you configure the scheduler, the "
        "health loop and the plumbing, you do not write them.",
        width=CONTENT_WIDTH,
    )
    return scene
