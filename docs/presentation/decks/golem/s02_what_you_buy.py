"""A service-model matrix whose row order puts the host OS under virtualisation.

There is an operating system on the metal first, and a hypervisor on top of it;
any system inside that hypervisor is a guest. That ordering is what makes each
column's boundary derivable rather than decorative, and it agrees with slide 02,
where "Container runtime" already sits above "Host OS & kernel".

`YOURS_DEPTH` follows from what each model sells, not from the row above it:

- own hardware, 8 — the building, the machines and everything on them
- colocation, 7 — the provider supplies the facility; the machines are yours
- rented bare metal, 5 — the provider supplies the facility, the machines and the
  network and storage under them; you install the system and any virtualisation
- IaaS, 3 — the provider also runs the machine's system and the hypervisor, so
  what you get is a guest and you start at your own runtime
- PaaS, 2 — the provider runs the runtime and middleware too
- SaaS, 0 — with Data hosted rather than theirs

The IaaS column is the one the row swap moved, and the note under the legend is
what stops it being read as "the provider patches my VM": the row is the system on
the metal, and the guest inside their virtualisation is still yours.
"""

from __future__ import annotations

from excalidraw.layout import badge, legend, matrix, note, slide_header
from excalidraw.palette import HOSTED, THEIRS, YOURS
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import BODY_SIZE

SLUG = "what-you-buy"
TITLE = "What you buy"

MARKER_Y = 132.0
MARKER_HEIGHT = 46.0
GRID_Y = 190.0
ROW_HEIGHT = 64.0
HEADER_HEIGHT = 88.0
ROW_LABEL_WIDTH = 340.0

ROW_LABELS = (
    "Data",
    "Application",
    "Runtime & middleware",
    "Virtualisation",
    "Operating system",
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

LICHESS_COLUMN = 2

YOURS_DEPTH = (8, 7, 5, 3, 2, 0)

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
    slide_header(scene, TITLE)
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
    bottom = legend(
        scene,
        MARGIN,
        grid.bottom + 26,
        (
            (YOURS, "you operate it"),
            (THEIRS, "the provider operates it"),
            (HOSTED, "yours, stored by the provider"),
        ),
    )
    note(
        scene,
        MARGIN,
        bottom + 18,
        "Operating system is the one on the metal. On IaaS the guest system inside "
        "the provider's virtualisation is yours.",
        width=CONTENT_WIDTH,
    )
    return scene
