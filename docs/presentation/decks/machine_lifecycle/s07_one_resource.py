from __future__ import annotations

from excalidraw.layout import badge, note, slide_header
from excalidraw.palette import INK_SOFT, PULUMI, WHITE, Tone
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.text import MONO
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE, HEADING_SIZE

from . import lifecycle

SLUG = "one-resource"
TITLE = "Steps 1 to 3 are one Pulumi resource"

SUBTITLE = "One resource orders the machine, installs the operating system and lays out the disks."

RESOURCE = "ovh.Dedicated.Server"
RESOURCE_Y = 208.0

ROWS_X = MARGIN
ROWS_Y = 276.0
ROW_HEIGHT = 148.0
ROW_GAP = 20.0
NAME_X = ROWS_X + 80.0
NAME_WIDTH = 380.0
FIELD_X = NAME_X + NAME_WIDTH
FIELD_GAP = 12.0

ROWS = (
    (1, "Order the machine", ("ovhSubsidiary", "plans", "planOptions", "range")),
    (2, "Install the operating system", ("os", "customizations")),
    (3, "Partition the disks", ("storages", "hardwareRaids", "partitionings")),
)

SOURCE_Y = 786.0
SOURCE = (
    "Pulumi's OVHcloud provider, Dedicated module. The same fields appear on "
    "ovh.Dedicated.ServerReinstallTask, which reinstalls a server already ordered."
)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, TITLE, SUBTITLE)
    scene.text(
        ROWS_X,
        RESOURCE_Y,
        RESOURCE,
        font_size=HEADING_SIZE,
        colour=PULUMI.stroke,
        font_family=MONO,
        width=CONTENT_WIDTH,
    )
    for position, (number, name, fields) in enumerate(ROWS):
        top = ROWS_Y + position * (ROW_HEIGHT + ROW_GAP)
        scene.rectangle(
            ROWS_X,
            top + (ROW_HEIGHT - lifecycle.NUMBER_SIZE) / 2.0,
            lifecycle.NUMBER_SIZE,
            lifecycle.NUMBER_SIZE,
            Tone(PULUMI.stroke, WHITE, PULUMI.stroke),
            label=str(number),
            label_font_size=BODY_SIZE,
        )
        note(
            scene,
            NAME_X,
            top + (ROW_HEIGHT - BODY_SIZE * 1.25) / 2.0,
            name,
            width=NAME_WIDTH - 24.0,
            font_size=BODY_SIZE,
        )
        cursor = FIELD_X
        for field in fields:
            chip = badge(
                scene,
                cursor,
                top + (ROW_HEIGHT - CAPTION_SIZE * 1.25 - 16.0) / 2.0,
                field,
                tone=PULUMI,
                font_family=MONO,
            )
            cursor += chip["width"] + FIELD_GAP
    note(
        scene,
        MARGIN,
        SOURCE_Y,
        SOURCE,
        width=CONTENT_WIDTH,
        font_size=CAPTION_SIZE,
        colour=INK_SOFT,
    )
    return scene
