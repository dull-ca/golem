from __future__ import annotations

from excalidraw.layout import callout, legend, slide_header
from excalidraw.palette import CONTAINER, HOSTED, PLATFORM, YOURS
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene, bottom_edge

from ..vocabulary import ORCHESTRATION_PARTS
from . import lichess_stack

SLUG = "bought-orchestration"
TITLE = "If we bought orchestration"

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
    1: "ours",
    2: PLATFORM_TAG,
    3: PLATFORM_TAG,
    4: IMAGE_TAG,
    5: IMAGE_TAG,
    6: PLATFORM_TAG,
}

PART_TONES = {part.number: PLATFORM for part in ORCHESTRATION_PARTS}

CALLOUT_Y = lichess_stack.BOTTOM + 16


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        "If we bought orchestration",
        "A platform provides all five parts of layer 6 together.",
    )
    lichess_stack.draw(
        scene,
        layer_tones=LAYER_TONES,
        layer_tags=LAYER_TAGS,
        part_tones=PART_TONES,
        show_details=False,
    )
    banner = callout(
        scene,
        MARGIN,
        CALLOUT_Y,
        CONTENT_WIDTH,
        "Renting managed Kubernetes covers layer 1 too — and costs more.",
        tone=HOSTED,
    )
    legend(
        scene,
        MARGIN,
        bottom_edge(banner) + 18,
        (
            (PLATFORM, "provided by the platform"),
            (CONTAINER, "provided by the image"),
            (YOURS, "ours to operate"),
        ),
    )
    return scene
