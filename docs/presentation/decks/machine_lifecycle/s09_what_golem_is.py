from __future__ import annotations

from excalidraw import icons
from excalidraw.layout import connector, note, panel, slide_header
from excalidraw.palette import GOLEM, INK_SOFT, NEUTRAL
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE

from .. import machines
from ..lichess_fleet import HOSTS

SLUG = "what-golem-is"
TITLE = "What golem is"

SUBTITLE = (
    "A typed program compiles to a manifest holding one scroll per host, and the "
    "agent on each host enacts its own scroll."
)

PROGRAM_X = MARGIN
PROGRAM_Y = 296.0
PROGRAM_SIZE = 150.0

MANIFEST_X = 430.0
MANIFEST_Y = 268.0
MANIFEST_WIDTH = 380.0
MANIFEST_HEIGHT = 300.0
SCROLL_WIDTH = 96.0
SCROLL_HEIGHT = 150.0
SCROLL_GAP = 18.0

HOSTS_X = 1030.0
HOSTS_Y = 268.0
HOST_WIDTH = 260.0
HOST_HEIGHT = 132.0
HOST_GAP = 16.0

SCROLL_HOSTS = tuple(
    host for host in HOSTS if host.name in ("orbit", "cobar", "dingo")
)

CLOSING_Y = 762.0
CLOSING = (
    "golemd records the prior state of everything it changes, so taking a unit out "
    "of the program takes it off the host. It reverses only what it wrote down."
)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, TITLE, SUBTITLE)
    icons.source_file(scene, PROGRAM_X + 30.0, PROGRAM_Y, PROGRAM_SIZE, tone=GOLEM)
    note(
        scene,
        PROGRAM_X,
        PROGRAM_Y + PROGRAM_SIZE + 16.0,
        "an Emet program",
        width=260.0,
        font_size=BODY_SIZE,
    )
    manifest = panel(
        scene,
        MANIFEST_X,
        MANIFEST_Y,
        MANIFEST_WIDTH,
        MANIFEST_HEIGHT,
        "the manifest",
        tone=NEUTRAL,
    )
    for position, host in enumerate(SCROLL_HOSTS):
        machines.scroll_mark(
            scene,
            manifest.body.x + position * (SCROLL_WIDTH + SCROLL_GAP),
            manifest.body.y + 12.0,
            SCROLL_WIDTH,
            SCROLL_HEIGHT,
            host.name,
            GOLEM,
        )
    for position, host in enumerate(SCROLL_HOSTS):
        machines.draw_machine(
            scene,
            HOSTS_X,
            HOSTS_Y + position * (HOST_HEIGHT + HOST_GAP),
            machines.Machine(
                host.name,
                tool_units=host.units,
                keeper=GOLEM,
                unit_tone=GOLEM,
                agent=True,
            ),
            width=HOST_WIDTH,
            height=HOST_HEIGHT,
            name_font_size=CAPTION_SIZE,
        )
    connector(
        scene,
        [
            (PROGRAM_X + 30.0 + PROGRAM_SIZE * icons.SOURCE_FILE_ASPECT + 16.0, PROGRAM_Y + PROGRAM_SIZE / 2.0),
            (MANIFEST_X - 16.0, PROGRAM_Y + PROGRAM_SIZE / 2.0),
        ],
        stroke=GOLEM.stroke,
        stroke_width=3,
        label="emetc",
    )
    connector(
        scene,
        [
            (MANIFEST_X + MANIFEST_WIDTH + 16.0, MANIFEST_Y + MANIFEST_HEIGHT / 2.0),
            (HOSTS_X - 16.0, MANIFEST_Y + MANIFEST_HEIGHT / 2.0),
        ],
        stroke=GOLEM.stroke,
        stroke_width=3,
        label="golemctl",
    )
    note(
        scene,
        MANIFEST_X,
        HOSTS_Y + 3 * (HOST_HEIGHT + HOST_GAP) + 6.0,
        "golemctl posts the manifest; each host picks the scroll that names it",
        width=CONTENT_WIDTH - MANIFEST_X + MARGIN,
        font_size=CAPTION_SIZE,
        colour=INK_SOFT,
    )
    note(scene, MARGIN, CLOSING_Y, CLOSING, width=CONTENT_WIDTH)
    return scene
