from __future__ import annotations

from excalidraw.layout import badge, legend, matrix, slide_header
from excalidraw.palette import PLATFORM, THEIRS, YOURS
from excalidraw.scene import MARGIN, Scene
from excalidraw.type_scale import BODY_SIZE

SLUG = "what-you-configure"
TITLE = "What you configure"

MARKER_Y = 132.0
MARKER_HEIGHT = 46.0
GRID_Y = 190.0
ROW_HEIGHT = 72.0
HEADER_HEIGHT = 88.0
ROW_LABEL_WIDTH = 380.0
LICHESS_COLUMN = 0

ROW_LABELS = (
    "App config & secrets",
    "Scaling policy",
    "Service discovery & load balancing",
    "Scheduling & placement",
    "Cluster membership",
    "Container runtime",
    "Host OS & kernel",
    "Hardware",
)

COLUMN_LABELS = (
    "Bare metal + config mgmt",
    "Docker (one host)",
    "Swarm",
    "Nomad",
    "Kubernetes",
    "Managed Kubernetes",
)

Y = YOURS
P = PLATFORM
T = THEIRS

CELL_TONES = (
    (Y, Y, Y, Y, Y, Y),
    (Y, Y, Y, Y, Y, Y),
    (Y, Y, P, P, P, P),
    (Y, Y, P, P, P, P),
    (T, T, P, P, P, T),
    (Y, P, P, P, P, T),
    (Y, Y, Y, Y, Y, T),
    (T, T, T, T, T, T),
)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, "What you configure")
    grid = matrix(
        scene,
        MARGIN,
        GRID_Y,
        column_labels=COLUMN_LABELS,
        row_labels=ROW_LABELS,
        tones=CELL_TONES,
        row_label_width=ROW_LABEL_WIDTH,
        header_height=HEADER_HEIGHT,
        row_height=ROW_HEIGHT,
    )
    marker_x = grid.column_x(LICHESS_COLUMN) + grid.column_width / 2.0
    badge(
        scene,
        marker_x,
        MARKER_Y,
        "lichess is here",
        tone=YOURS,
        font_size=BODY_SIZE,
        anchor="center",
        height=MARKER_HEIGHT,
    )
    legend(
        scene,
        MARGIN,
        grid.bottom + 24,
        (
            (YOURS, "you configure it"),
            (PLATFORM, "the platform provides it"),
            (THEIRS, "you purchased, and can't configure"),
        ),
    )
    return scene
