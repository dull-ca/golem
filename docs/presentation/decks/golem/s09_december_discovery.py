from __future__ import annotations

from excalidraw import icons
from excalidraw.layout import IconCard, icon_card_row, slide_header, span_bar
from excalidraw.palette import BLUE, INK_FAINT, PLATFORM, WHITE, WIRE, WORKLOAD, Tone
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import BODY_SIZE

SLUG = "december-discovery"
TITLE = "December: service discovery"

CARDS_Y = 200.0
CARD_HEIGHT = 380.0
ICON_SIZE = 140.0
CLOSING_Y = 700.0
CLOSING_HEIGHT = 58.0

CARD_TONE = Tone(BLUE, WHITE)
EDGE_TONE = Tone(INK_FAINT, WHITE)

CARDS = (
    IconCard(
        icons.network_link,
        icons.NETWORK_LINK_ASPECT,
        "OVH vrack",
        "a private L2 between the rented machines",
        EDGE_TONE,
        WIRE,
    ),
    IconCard(
        icons.dns_lookup,
        icons.DNS_LOOKUP_ASPECT,
        "dnsmasq",
        "one resolver per host",
        CARD_TONE,
        WIRE,
    ),
    IconCard(
        icons.service,
        icons.SERVICE_ASPECT,
        "SRV records",
        "a name resolves to a host and a port",
        CARD_TONE,
        WIRE,
    ),
    IconCard(
        icons.container,
        icons.CONTAINER_ASPECT,
        "Clients",
        "ask for the name, not the box",
        EDGE_TONE,
        WORKLOAD,
    ),
)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        "December: how a client found a service",
        "A private network, and DNS that names services.",
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
        CLOSING_Y,
        CONTENT_WIDTH,
        "A client resolves a service, never a machine.",
        tone=PLATFORM,
        height=CLOSING_HEIGHT,
    )
    return scene
