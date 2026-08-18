from __future__ import annotations

from excalidraw import icons
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
NOT_APPLICABLE_MARK_SIZE = 48.0

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

# NOTE: single-host models have no cluster for a host to be a member of, so
# the row doesn't apply — a different claim from "purchased, can't configure"
# (Managed Kubernetes stays grey and unmarked: a provider-run control plane
# makes that claim true there).
CLUSTER_MEMBERSHIP_ROW = ROW_LABELS.index("Cluster membership")
NO_CLUSTER_COLUMNS = (
    COLUMN_LABELS.index("Bare metal + config mgmt"),
    COLUMN_LABELS.index("Docker (one host)"),
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
    for column in NO_CLUSTER_COLUMNS:
        area = grid.cell(CLUSTER_MEMBERSHIP_ROW, column)
        icons.not_applicable(
            scene,
            area.x + (area.width - NOT_APPLICABLE_MARK_SIZE) / 2.0,
            area.y + (area.height - NOT_APPLICABLE_MARK_SIZE) / 2.0,
            NOT_APPLICABLE_MARK_SIZE,
        )
    legend(
        scene,
        MARGIN,
        grid.bottom + 24,
        (
            (YOURS, "you configure it"),
            (PLATFORM, "the platform provides it"),
            (THEIRS, "you purchased, and can't configure"),
            (THEIRS, "this model has no cluster"),
        ),
        marks=(None, None, None, icons.not_applicable),
    )
    return scene
