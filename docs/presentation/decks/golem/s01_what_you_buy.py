from __future__ import annotations

from excalidraw.layout import legend, matrix, slide_header
from excalidraw.palette import HOSTED, THEIRS, YOURS
from excalidraw.scene import MARGIN, Scene

SLUG = "what-you-buy"
TITLE = "What you buy"

GRID_Y = 190.0
ROW_HEIGHT = 64.0
HEADER_HEIGHT = 88.0
ROW_LABEL_WIDTH = 340.0

ROW_LABELS = (
    "Data",
    "Application",
    "Runtime & middleware",
    "Operating system",
    "Virtualisation",
    "Network & storage",
    "Hardware",
    "Facility & power",
)

COLUMN_LABELS = (
    "Own hardware",
    "Colocation",
    "Rented bare metal",
    "IaaS (cloud VMs)",
    "PaaS",
    "SaaS",
)

YOURS_DEPTH = (8, 7, 5, 4, 2, 0)

HOSTED_CELLS = frozenset({(0, 5)})


def cell_tones():
    return [
        [
            HOSTED
            if (row, column) in HOSTED_CELLS
            else (YOURS if row < YOURS_DEPTH[column] else THEIRS)
            for column in range(len(COLUMN_LABELS))
        ]
        for row in range(len(ROW_LABELS))
    ]


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, "What you buy")
    grid = matrix(
        scene,
        MARGIN,
        GRID_Y,
        column_labels=COLUMN_LABELS,
        row_labels=ROW_LABELS,
        tones=cell_tones(),
        row_label_width=ROW_LABEL_WIDTH,
        header_height=HEADER_HEIGHT,
        row_height=ROW_HEIGHT,
    )
    legend(
        scene,
        MARGIN,
        grid.bottom + 26,
        (
            (YOURS, "you operate it"),
            (THEIRS, "the provider operates it"),
            (HOSTED, "yours, stored by the provider"),
        ),
    )
    return scene
