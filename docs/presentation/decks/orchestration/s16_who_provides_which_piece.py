from __future__ import annotations

from excalidraw.layout import legend, matrix, note, slide_header
from excalidraw.palette import PLATFORM, THEIRS, YOURS
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene

SLUG = "who-provides-which-piece"
TITLE = "Who provides which piece"

GRID_Y = 190.0
ROW_HEIGHT = 66.0
HEADER_HEIGHT = 88.0
ROW_LABEL_WIDTH = 460.0

ROW_LABELS = (
    "Container runtime",
    "Cluster membership",
    "Placement",
    "Lifecycle",
    "Health and reconciliation",
    "Supporting plumbing",
    "Scaling",
    "Storage and secrets",
)

COLUMN_LABELS = ("Docker (one host)", "Swarm", "Nomad", "Kubernetes")

P = PLATFORM
Y = YOURS
T = THEIRS

CELL_TONES = (
    (P, P, P, P),
    (T, P, P, P),
    (Y, P, P, P),
    (P, P, P, P),
    (Y, P, P, P),
    (Y, P, Y, P),
    (Y, P, P, P),
    (Y, P, Y, P),
)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, "Who provides which piece")
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
            (PLATFORM, "provided by the platform"),
            (YOURS, "you provide it"),
            (THEIRS, "no such thing here"),
        ),
    )
    note(
        scene,
        MARGIN,
        grid.bottom + 74,
        "Nomad leaves plumbing and secrets to Consul and Vault.",
        width=CONTENT_WIDTH,
    )
    return scene
