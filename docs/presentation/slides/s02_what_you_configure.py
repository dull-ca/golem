from __future__ import annotations

from excalidraw.layout import legend, matrix, note, slide_header, span_bar
from excalidraw.palette import BESPOKE, PLATFORM, THEIRS, YOURS
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene

SLUG = "what-you-configure"
TITLE = "What you configure"

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

LICHESS_COLUMN = 0
PORTAINER_FIRST_COLUMN = 1
PORTAINER_LAST_COLUMN = 4


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        "The ladder of what you configure",
        "Same shape, different question: not what you buy, what you still have to answer.",
    )
    note(
        scene,
        MARGIN,
        150,
        "Past the middle you stop thinking in machines and start thinking in resources.",
        width=CONTENT_WIDTH,
    )
    grid = matrix(
        scene,
        MARGIN,
        188,
        column_labels=COLUMN_LABELS,
        row_labels=ROW_LABELS,
        tones=CELL_TONES,
        row_label_width=250,
        header_height=64,
        row_height=54,
    )
    annotation_y = grid.bottom + 18
    lichess_span = grid.column_span(LICHESS_COLUMN, LICHESS_COLUMN)
    span_bar(
        scene,
        lichess_span.x,
        annotation_y,
        lichess_span.width,
        "lichess is here",
        tone=YOURS,
        font_size=15,
        height=40,
    )
    note(
        scene,
        lichess_span.x,
        annotation_y + 46,
        "configured, rented bare metal",
        width=lichess_span.width,
        font_size=13,
        align="center",
    )
    portainer_span = grid.column_span(PORTAINER_FIRST_COLUMN, PORTAINER_LAST_COLUMN)
    span_bar(
        scene,
        portainer_span.x,
        annotation_y,
        portainer_span.width,
        "Portainer — a configurator that sits on top of these, not another layer of the stack",
        tone=BESPOKE,
        font_size=14,
        height=40,
    )
    legend(
        scene,
        MARGIN,
        annotation_y + 104,
        (
            (YOURS, "yours — you configure it"),
            (PLATFORM, "platform — the platform answers it"),
            (THEIRS, "theirs — not your problem"),
        ),
    )
    return scene
