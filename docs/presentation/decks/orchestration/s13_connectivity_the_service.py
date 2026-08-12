from __future__ import annotations

from excalidraw import icons
from excalidraw.layout import IconCard, icon_card_row, slide_header, span_bar
from excalidraw.palette import BLUE, CONTROL, WHITE, WIRE, Tone
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import BODY_SIZE, HEADING_SIZE

SLUG = "connectivity-the-service"
TITLE = "Connectivity: the service"

CARDS_Y = 240.0
CARD_HEIGHT = 390.0
ICON_SIZE = 190.0

CLOSING_Y = 700.0
CLOSING_HEIGHT = 62.0

CARD_TONE = Tone(BLUE, WHITE)

CARDS = (
    IconCard(
        icons.dns_lookup,
        icons.DNS_LOOKUP_ASPECT,
        "DNS or SRV",
        "a name resolves to a host and a port",
        CARD_TONE,
        WIRE,
    ),
    IconCard(
        icons.service,
        icons.SERVICE_ASPECT,
        "The service",
        "one stable name, a moving set of instances",
        CARD_TONE,
        WIRE,
    ),
    IconCard(
        icons.load_balancer,
        icons.LOAD_BALANCER_ASPECT,
        "Load balancer",
        "instances register, traffic follows",
        CARD_TONE,
        CONTROL,
    ),
)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        "Connectivity: the service",
        "A stable name in front of a moving set of instances.",
    )
    icon_card_row(
        scene,
        MARGIN,
        CARDS_Y,
        CARDS,
        card_height=CARD_HEIGHT,
        icon_size=ICON_SIZE,
        flow=True,
        title_font_size=HEADING_SIZE,
        detail_font_size=BODY_SIZE,
    )
    span_bar(
        scene,
        MARGIN,
        CLOSING_Y,
        CONTENT_WIDTH,
        "Clients resolve a service, never a machine.",
        tone=WIRE,
        height=CLOSING_HEIGHT,
    )
    return scene
