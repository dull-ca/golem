from __future__ import annotations

from excalidraw import icons
from excalidraw.layout import IconCard, icon_card_row, slide_header, span_bar
from excalidraw.palette import (
    BESPOKE,
    GRAPE,
    NODE,
    ORANGE,
    STORE,
    VIOLET,
    WHITE,
    WORKLOAD,
    Tone,
)
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import BODY_SIZE

SLUG = "december-placement"
TITLE = "December: how a service reached a host"

CARDS_Y = 200.0
CARD_HEIGHT = 380.0
ICON_SIZE = 130.0
CLOSING_Y = 700.0
CLOSING_HEIGHT = 58.0

BESPOKE_CARD = Tone(ORANGE, WHITE)
ANSIBLE_CARD = Tone(GRAPE, WHITE)
SYSTEMD_CARD = Tone(VIOLET, WHITE)

CARDS = (
    IconCard(
        icons.binding,
        icons.BINDING_ASPECT,
        "hosts.py",
        "which service runs on which host, written down",
        BESPOKE_CARD,
    ),
    IconCard(
        icons.registry,
        icons.REGISTRY_ASPECT,
        "Generated config",
        "Ansible inventory and quadlet variables",
        ANSIBLE_CARD,
        STORE,
    ),
    IconCard(
        icons.container,
        icons.CONTAINER_ASPECT,
        "systemd quadlets",
        "podman units written onto the host",
        SYSTEMD_CARD,
        WORKLOAD,
    ),
    IconCard(
        icons.host,
        icons.HOST_ASPECT,
        "Lifecycle",
        "start, stop and restart, from systemd",
        SYSTEMD_CARD,
        NODE,
    ),
)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, TITLE)
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
        CLOSING_Y,
        CONTENT_WIDTH,
        "A person chose which host ran each service.",
        tone=BESPOKE,
        height=CLOSING_HEIGHT,
    )
    return scene
