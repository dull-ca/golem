from __future__ import annotations

from excalidraw import icons
from excalidraw.layout import IconCard, icon_card_row, slide_header
from excalidraw.palette import BLUE, INK_FAINT, WHITE, WIRE, WORKLOAD, Tone
from excalidraw.scene import MARGIN, Scene
from excalidraw.type_scale import BODY_SIZE

SLUG = "december-discovery"
TITLE = "December: how a client found a service"

CARDS_Y = 200.0
CARD_HEIGHT = 380.0
ICON_SIZE = 140.0

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
        "look up the name, connect to what it returns",
        EDGE_TONE,
        WORKLOAD,
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
    return scene
