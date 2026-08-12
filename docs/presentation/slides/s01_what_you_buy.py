from __future__ import annotations

from excalidraw.layout import legend, matrix, note, slide_header
from excalidraw.palette import HOSTED, THEIRS, YOURS
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene

SLUG = "what-you-buy"
TITLE = "What you buy"

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
    slide_header(
        scene,
        "The ladder of what you buy",
        "One stack, eight layers. Every service model is a line drawn across it — the "
        "line just moves.",
    )
    grid = matrix(
        scene,
        MARGIN,
        168,
        column_labels=COLUMN_LABELS,
        row_labels=ROW_LABELS,
        tones=cell_tones(),
        row_label_width=250,
        header_height=64,
        row_height=58,
    )
    legend(
        scene,
        MARGIN,
        grid.bottom + 30,
        (
            (YOURS, "yours — you own it and you operate it"),
            (THEIRS, "theirs — you never touch it"),
            (HOSTED, "hosted — yours, but on their terms"),
        ),
    )
    note(
        scene,
        MARGIN,
        grid.bottom + 84,
        "Left to right: the less you own, the less you control. Nothing here is free — "
        "what you stop operating, you start depending on.",
        width=CONTENT_WIDTH,
    )
    return scene
