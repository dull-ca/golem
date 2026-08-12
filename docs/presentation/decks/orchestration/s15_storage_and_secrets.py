from __future__ import annotations

from excalidraw import icons
from excalidraw.layout import note, slide_header, span_bar, split_compare
from excalidraw.palette import IMAGE, STORE
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import BODY_SIZE

SLUG = "storage-and-secrets"
TITLE = "Storage and secrets"

PANELS_Y = 200.0
PANELS_HEIGHT = 520.0

MARK_SIZE = 200.0
MARK_Y = 295.0

FIRST_NOTE_Y = 520.0
SECOND_NOTE_Y = 575.0

CLOSING_Y = 770.0
CLOSING_HEIGHT = 62.0


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        "Storage and secrets",
        "Two things that have to follow the workload.",
    )
    volumes, secrets = split_compare(
        scene,
        MARGIN,
        PANELS_Y,
        CONTENT_WIDTH,
        PANELS_HEIGHT,
        ("Volumes", STORE),
        ("Secrets and config", IMAGE),
    )
    icons.volume(
        scene,
        volumes.body.x
        + (volumes.body.width - icons.VOLUME_ASPECT * MARK_SIZE) / 2.0,
        MARK_Y,
        MARK_SIZE,
    )
    note(
        scene,
        volumes.body.x,
        FIRST_NOTE_Y,
        "A container's filesystem dies with it.",
        width=volumes.body.width,
        font_size=BODY_SIZE,
        align="center",
    )
    note(
        scene,
        volumes.body.x,
        SECOND_NOTE_Y,
        "An attached volume constrains placement.",
        width=volumes.body.width,
        font_size=BODY_SIZE,
        align="center",
    )
    icons.secret(
        scene,
        secrets.body.x
        + (secrets.body.width - icons.SECRET_ASPECT * MARK_SIZE) / 2.0,
        MARK_Y,
        MARK_SIZE,
        tone=IMAGE,
    )
    note(
        scene,
        secrets.body.x,
        FIRST_NOTE_Y,
        "Delivered to the node that runs it.",
        width=secrets.body.width,
        font_size=BODY_SIZE,
        align="center",
    )
    note(
        scene,
        secrets.body.x,
        SECOND_NOTE_Y,
        "Mounted or injected, rotated without a rebuild.",
        width=secrets.body.width,
        font_size=BODY_SIZE,
        align="center",
    )
    span_bar(
        scene,
        MARGIN,
        CLOSING_Y,
        CONTENT_WIDTH,
        "Both follow the workload to whichever node runs it.",
        tone=STORE,
        height=CLOSING_HEIGHT,
    )
    return scene
