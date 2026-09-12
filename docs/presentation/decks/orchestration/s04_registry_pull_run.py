from __future__ import annotations

from excalidraw import icons
from excalidraw.layout import IconCard, icon_card_row, slide_header, span_bar
from excalidraw.palette import (
    GRAPE,
    IMAGE,
    NODE,
    PLATFORM,
    SLATE,
    STORE,
    TEAL,
    VIOLET,
    WHITE,
    WORKLOAD,
    Tone,
)
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import BODY_SIZE

SLUG = "registry-pull-run"
TITLE = "Registry, pull, run"

CARDS_Y = 196.0
CARD_HEIGHT = 410.0
ICON_SIZE = 200.0

BAR_Y = 670.0
BAR_HEIGHT = 64.0

CARDS = (
    IconCard(
        icons.container_image,
        icons.CONTAINER_IMAGE_ASPECT,
        "Build",
        "layers written once",
        Tone(VIOLET, WHITE),
        IMAGE,
    ),
    IconCard(
        icons.registry,
        icons.REGISTRY_ASPECT,
        "Push",
        "the registry keeps the bytes",
        Tone(GRAPE, WHITE),
        STORE,
    ),
    IconCard(
        icons.host,
        icons.HOST_ASPECT,
        "Pull",
        "a host fetches it by digest",
        Tone(SLATE, WHITE),
        NODE,
    ),
    IconCard(
        icons.container,
        icons.CONTAINER_ASPECT,
        "Run",
        "the runtime starts it",
        Tone(TEAL, WHITE),
        WORKLOAD,
    ),
)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        "How an image reaches a machine",
        "A registry is a server that stores images and serves them by digest.",
    )
    icon_card_row(
        scene,
        MARGIN,
        CARDS_Y,
        CARDS,
        card_height=CARD_HEIGHT,
        icon_size=ICON_SIZE,
        flow=True,
        detail_font_size=BODY_SIZE,
    )
    span_bar(
        scene,
        MARGIN,
        BAR_Y,
        CONTENT_WIDTH,
        "The registry is the only thing the host must reach.",
        tone=PLATFORM,
        height=BAR_HEIGHT,
    )
    return scene
