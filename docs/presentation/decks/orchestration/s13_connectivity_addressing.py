from __future__ import annotations

from excalidraw import icons
from excalidraw.layout import note, slide_header, split_compare
from excalidraw.palette import NODE, WIRE, WORKLOAD
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import BODY_SIZE

SLUG = "connectivity-addressing"
TITLE = "Connectivity: addressing"

LEAD_Y = 200.0

PANELS_Y = 270.0
PANELS_HEIGHT = 460.0

HOST_SIZE = 150.0
HOST_Y = 480.0
SHARED_CONTAINER_SIZE = 90.0
SHARED_CONTAINER_Y = 370.0

LINK_SIZE = 130.0
LINK_Y = 370.0
OVERLAY_CONTAINER_SIZE = 90.0
OVERLAY_CONTAINER_Y = 520.0
OVERLAY_CONTAINER_GAP = 60.0

PANEL_NOTE_Y = 650.0


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, "Connectivity: addressing")
    note(
        scene,
        MARGIN,
        LEAD_Y,
        "Every container gets an address. It does not survive a restart or a move.",
        width=CONTENT_WIDTH,
    )
    shared, overlay = split_compare(
        scene,
        MARGIN,
        PANELS_Y,
        CONTENT_WIDTH,
        PANELS_HEIGHT,
        ("Host networking", NODE),
        ("Overlay network", WIRE),
    )
    shared_centre = shared.body.x + shared.body.width / 2.0
    icons.container(
        scene,
        shared_centre - icons.CONTAINER_ASPECT * SHARED_CONTAINER_SIZE / 2.0,
        SHARED_CONTAINER_Y,
        SHARED_CONTAINER_SIZE,
        tone=WORKLOAD,
    )
    icons.host(
        scene,
        shared_centre - icons.HOST_ASPECT * HOST_SIZE / 2.0,
        HOST_Y,
        HOST_SIZE,
    )
    note(
        scene,
        shared.body.x,
        PANEL_NOTE_Y,
        "The host's address, and its ports.",
        width=shared.body.width,
        font_size=BODY_SIZE,
        align="center",
    )
    overlay_centre = overlay.body.x + overlay.body.width / 2.0
    icons.network_link(
        scene,
        overlay_centre - icons.NETWORK_LINK_ASPECT * LINK_SIZE / 2.0,
        LINK_Y,
        LINK_SIZE,
    )
    overlay_container_width = icons.CONTAINER_ASPECT * OVERLAY_CONTAINER_SIZE
    pair_span = 2 * overlay_container_width + OVERLAY_CONTAINER_GAP
    for index in range(2):
        icons.container(
            scene,
            overlay_centre
            - pair_span / 2.0
            + index * (overlay_container_width + OVERLAY_CONTAINER_GAP),
            OVERLAY_CONTAINER_Y,
            OVERLAY_CONTAINER_SIZE,
            tone=WORKLOAD,
        )
    note(
        scene,
        overlay.body.x,
        PANEL_NOTE_Y,
        "Its own address, across hosts.",
        width=overlay.body.width,
        font_size=BODY_SIZE,
        align="center",
    )
    return scene
