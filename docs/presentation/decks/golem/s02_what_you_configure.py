from __future__ import annotations

from excalidraw.layout import legend, matrix, slide_header
from excalidraw.palette import PLATFORM, THEIRS, YOURS
from excalidraw.scene import MARGIN, Scene

SLUG = "what-you-configure"
TITLE = "What you configure"

GRID_Y = 190.0
ROW_HEIGHT = 72.0
HEADER_HEIGHT = 88.0
ROW_LABEL_WIDTH = 380.0

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
    legend(
        scene,
        MARGIN,
        grid.bottom + 24,
        (
            (YOURS, "you configure it"),
            (PLATFORM, "the platform provides it"),
            (THEIRS, "not yours to configure"),
        ),
    )
    return scene
