from __future__ import annotations

from excalidraw.layout import LabelledBox, box_column, note, slide_header
from excalidraw.palette import ANSIBLE, INK_SOFT, NEUTRAL
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE

from .. import machines
from ..lichess_fleet import HAND_UNIT_COUNT, HOSTS, TOOL_UNIT_COUNT, UNIT_COUNT

SLUG = "the-services"
TITLE = "Step 5: the services, by hand"

SUBTITLE = "What a machine runs, and how each part of it is configured, is decided on that machine."

KINDS_X = MARGIN
KINDS_Y = 268.0
KINDS_WIDTH = 400.0
KIND_HEIGHT = 72.0
KIND_GAP = 14.0

HOST_X = 700.0
HOST_Y = 268.0
HOST_WIDTH = 520.0
HOST_HEIGHT = 280.0

LEGEND_Y = 580.0
BORDER_NOTE_Y = 632.0

KINDS = ("services", "ingress entries", "databases", "workloads")

EXAMPLE = next(host for host in HOSTS if host.name == "talos")

CLOSING_Y = 720.0
CLOSING = (
    f"The inventory records {UNIT_COUNT} units across the thirty machines. A tool "
    f"keeps {TOOL_UNIT_COUNT} of them. The other {HAND_UNIT_COUNT} are on a host "
    "and nothing keeps them."
)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, TITLE, SUBTITLE)
    note(
        scene,
        KINDS_X,
        KINDS_Y,
        "A unit on a host is one of",
        width=KINDS_WIDTH,
        font_size=BODY_SIZE,
    )
    box_column(
        scene,
        KINDS_X,
        KINDS_Y + 48.0,
        KINDS_WIDTH,
        tuple(LabelledBox(kind, "", NEUTRAL) for kind in KINDS),
        box_height=KIND_HEIGHT,
        gap=KIND_GAP,
    )
    machines.draw_machine(
        scene,
        HOST_X,
        HOST_Y,
        machines.Machine(
            EXAMPLE.name,
            hand_units=EXAMPLE.hand_units,
            unknown=EXAMPLE.unknown,
            keeper=ANSIBLE,
        ),
        width=HOST_WIDTH,
        height=HOST_HEIGHT,
        name_font_size=BODY_SIZE,
    )
    machines.hand_unit_entry(scene, HOST_X, LEGEND_Y)
    note(
        scene,
        HOST_X,
        BORDER_NOTE_Y,
        "The border stays Ansible's: it did the machine-level work in step 4.",
        width=CONTENT_WIDTH - HOST_X + MARGIN,
        font_size=CAPTION_SIZE,
        colour=INK_SOFT,
    )
    note(scene, MARGIN, CLOSING_Y, CLOSING, width=CONTENT_WIDTH)
    return scene
