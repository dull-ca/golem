from __future__ import annotations

from excalidraw import icons
from excalidraw.layout import (
    ClusterNode,
    IconCard,
    cluster_map,
    connector,
    icon_card_row,
    note,
    slide_header,
    span_bar,
)
from excalidraw.palette import (
    BLUE,
    CONTROL,
    GRAPE,
    INK_FAINT,
    SLATE,
    STORE,
    WHITE,
    WORKLOAD,
    Tone,
)
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import BODY_SIZE

SLUG = "many-hosts-the-cluster"
TITLE = "Many hosts: the cluster"

PLANE_X = 310.0
PLANE_Y = 190.0
PLANE_CARD_WIDTH = 460.0
PLANE_CARD_GAP = 60.0
PLANE_CARD_HEIGHT = 250.0
PLANE_ICON_SIZE = 100.0

FEED_PATH = (
    (1060.0, 448.0),
    (1060.0, 480.0),
    (800.0, 480.0),
    (800.0, 504.0),
)

WORKERS_CAPTION_Y = 466.0
WORKERS_CAPTION_WIDTH = 620.0

MAP_Y = 512.0
NODE_HEIGHT = 190.0

BAR_Y = 760.0
BAR_HEIGHT = 64.0

NODE_TONE = Tone(SLATE, WHITE)

PLANE_CARDS = (
    IconCard(
        icons.volume,
        icons.VOLUME_ASPECT,
        "Desired state",
        "what you asked for",
        Tone(GRAPE, WHITE),
        STORE,
    ),
    IconCard(
        icons.scheduler,
        icons.SCHEDULER_ASPECT,
        "Control plane",
        "which host runs what",
        Tone(BLUE, WHITE),
        CONTROL,
    ),
)

WORKERS = (
    ClusterNode("host-1", 3, NODE_TONE, workload_tone=WORKLOAD),
    ClusterNode("host-2", 2, NODE_TONE, workload_tone=WORKLOAD),
    ClusterNode("host-3", 4, NODE_TONE, workload_tone=WORKLOAD),
    ClusterNode("host-4", 3, NODE_TONE, workload_tone=WORKLOAD),
)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        "Many hosts: the cluster",
        "A cluster is many hosts, a store of the state you want, and a control plane.",
    )
    icon_card_row(
        scene,
        PLANE_X,
        PLANE_Y,
        PLANE_CARDS,
        card_height=PLANE_CARD_HEIGHT,
        icon_size=PLANE_ICON_SIZE,
        card_width=PLANE_CARD_WIDTH,
        gap=PLANE_CARD_GAP,
        flow=True,
        detail_font_size=BODY_SIZE,
    )
    connector(scene, FEED_PATH, stroke=INK_FAINT)
    note(
        scene,
        MARGIN,
        WORKERS_CAPTION_Y,
        "Workers run what they are given.",
        width=WORKERS_CAPTION_WIDTH,
    )
    cluster_map(
        scene,
        MARGIN,
        MAP_Y,
        CONTENT_WIDTH,
        WORKERS,
        node_height=NODE_HEIGHT,
    )
    span_bar(
        scene,
        MARGIN,
        BAR_Y,
        CONTENT_WIDTH,
        "You name what should run. The control plane chooses where.",
        tone=CONTROL,
        height=BAR_HEIGHT,
    )
    return scene
