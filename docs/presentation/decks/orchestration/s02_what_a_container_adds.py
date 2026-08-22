from __future__ import annotations

from excalidraw import icons
from excalidraw.layout import IconCard, badge, icon_card_row, note, slide_header
from excalidraw.palette import (
    BESPOKE,
    NEUTRAL,
    NODE,
    ORANGE,
    SLATE,
    TEAL,
    VIOLET,
    WHITE,
    IMAGE,
    WORKLOAD,
    Tone,
)
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE

SLUG = "what-a-container-adds"
TITLE = "What a container adds"

BAND_Y = 190.0
BAND_HEIGHT = 200.0

HOST_X = 112.0
HOST_Y = 214.0
HOST_SIZE = 126.0
HOST_BADGE_Y = 344.0

CONTAINER_Y = 232.0
CONTAINER_SIZE = 84.0
CONTAINER_XS = (470.0, 620.0, 770.0)

ASIDE_X = 960.0
ASIDE_Y = 250.0
ASIDE_WIDTH = 520.0

CARDS_Y = 430.0
CARD_HEIGHT = 340.0
ICON_SIZE = 150.0

CARDS = (
    IconCard(
        icons.container_image,
        icons.CONTAINER_IMAGE_ASPECT,
        "Image",
        "its own filesystem",
        Tone(VIOLET, WHITE),
        IMAGE,
    ),
    IconCard(
        icons.container,
        icons.CONTAINER_ASPECT,
        "Namespaces",
        "its own view of processes, network, mounts",
        Tone(TEAL, WHITE),
        WORKLOAD,
    ),
    IconCard(
        icons.replica_set,
        icons.REPLICA_SET_ASPECT,
        "cgroups",
        "a bounded share of CPU and memory",
        Tone(ORANGE, WHITE),
        BESPOKE,
    ),
)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        "What a container adds",
        "A container is a process on the host, given three things it did not have.",
    )
    scene.rectangle(MARGIN, BAND_Y, CONTENT_WIDTH, BAND_HEIGHT, NEUTRAL)
    icons.host(scene, HOST_X, HOST_Y, HOST_SIZE, tone=NODE)
    badge(
        scene,
        HOST_X,
        HOST_BADGE_Y,
        "one kernel",
        tone=Tone(SLATE, WHITE),
        font_size=CAPTION_SIZE,
    )
    for left in CONTAINER_XS:
        icons.container(scene, left, CONTAINER_Y, CONTAINER_SIZE, tone=WORKLOAD)
    note(
        scene,
        ASIDE_X,
        ASIDE_Y,
        "These share the host's kernel. A virtual machine does not.",
        width=ASIDE_WIDTH,
    )
    icon_card_row(
        scene,
        MARGIN,
        CARDS_Y,
        CARDS,
        card_height=CARD_HEIGHT,
        icon_size=ICON_SIZE,
        detail_font_size=BODY_SIZE,
    )
    return scene
