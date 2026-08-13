from __future__ import annotations

from excalidraw import icons
from excalidraw.layout import connector, note, panel, slide_header
from excalidraw.palette import INK_SOFT, PULUMI, STORE, THEIRS
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE

SLUG = "what-pulumi-is"
TITLE = "What Pulumi is"

SUBTITLE = (
    "Pulumi is a program that declares the resources you want at a provider, and an "
    "engine that makes the provider match it."
)

PROGRAM_X = MARGIN
PROGRAM_Y = 300.0
PROGRAM_SIZE = 180.0
PROGRAM_WIDTH = 236.0

ENGINE_X = 360.0
ENGINE_Y = 300.0
ENGINE_WIDTH = 280.0
ENGINE_HEIGHT = 180.0
ENGINE_CENTRE_X = ENGINE_X + ENGINE_WIDTH / 2.0

STATE_SIZE = 150.0
STATE_Y = 566.0
STATE_WIDTH = STATE_SIZE * icons.VOLUME_ASPECT
STATE_X = ENGINE_CENTRE_X - STATE_WIDTH / 2.0

PROVIDER_X = 900.0
PROVIDER_Y = 268.0
PROVIDER_WIDTH = 636.0
PROVIDER_HEIGHT = 252.0
PROVIDER_MACHINE_SIZE = 104.0
PROVIDER_MACHINE_GAP = 22.0

SEND_Y = 356.0
READ_Y = 444.0

CLOSING_Y = 790.0
CLOSING = (
    "The engine reads the provider, compares it against the declaration and its own "
    "record of what it made last time, and changes only the difference."
)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, TITLE, SUBTITLE)
    program_width = PROGRAM_SIZE * icons.SOURCE_FILE_ASPECT
    icons.source_file(
        scene,
        PROGRAM_X + (PROGRAM_WIDTH - program_width) / 2.0,
        PROGRAM_Y,
        PROGRAM_SIZE,
        tone=PULUMI,
    )
    note(
        scene,
        PROGRAM_X,
        PROGRAM_Y + PROGRAM_SIZE + 18.0,
        "the machines you want",
        width=PROGRAM_WIDTH,
        font_size=BODY_SIZE,
        align="center",
    )
    scene.rectangle(
        ENGINE_X,
        ENGINE_Y,
        ENGINE_WIDTH,
        ENGINE_HEIGHT,
        PULUMI,
        label="pulumi up",
        label_font_size=BODY_SIZE,
    )
    icons.volume(scene, STATE_X, STATE_Y, STATE_SIZE, tone=STORE)
    note(
        scene,
        ENGINE_X,
        STATE_Y + STATE_SIZE + 16.0,
        "what it made last time",
        width=ENGINE_WIDTH,
        font_size=CAPTION_SIZE,
        colour=INK_SOFT,
        align="center",
    )
    provider = panel(
        scene,
        PROVIDER_X,
        PROVIDER_Y,
        PROVIDER_WIDTH,
        PROVIDER_HEIGHT,
        "OVH",
        tone=THEIRS,
        stroke_style="dashed",
    )
    machine_width = PROVIDER_MACHINE_SIZE * icons.HOST_ASPECT
    for position in range(3):
        icons.host(
            scene,
            provider.body.x + position * (machine_width + PROVIDER_MACHINE_GAP),
            provider.body.y + 8.0,
            PROVIDER_MACHINE_SIZE,
        )
    connector(
        scene,
        [
            (PROGRAM_X + (PROGRAM_WIDTH + program_width) / 2.0 + 18.0, PROGRAM_Y + PROGRAM_SIZE / 2.0),
            (ENGINE_X - 18.0, PROGRAM_Y + PROGRAM_SIZE / 2.0),
        ],
        stroke=PULUMI.stroke,
        stroke_width=3,
    )
    connector(
        scene,
        [(ENGINE_X + ENGINE_WIDTH + 18.0, SEND_Y), (PROVIDER_X - 18.0, SEND_Y)],
        stroke=PULUMI.stroke,
        stroke_width=3,
        label="create, change, destroy",
    )
    connector(
        scene,
        [(PROVIDER_X - 18.0, READ_Y), (ENGINE_X + ENGINE_WIDTH + 18.0, READ_Y)],
        stroke=INK_SOFT,
        stroke_width=2,
        dashed=True,
        label="what is actually there",
    )
    connector(
        scene,
        [
            (ENGINE_CENTRE_X, ENGINE_Y + ENGINE_HEIGHT + 18.0),
            (ENGINE_CENTRE_X, STATE_Y - 18.0),
        ],
        stroke=STORE.stroke,
        stroke_width=2,
        start_arrowhead="arrow",
    )
    note(scene, MARGIN, CLOSING_Y, CLOSING, width=CONTENT_WIDTH)
    return scene
