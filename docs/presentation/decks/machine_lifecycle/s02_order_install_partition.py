from __future__ import annotations

from excalidraw import icons
from excalidraw.layout import IconCard, icon_card_row, note, slide_header
from excalidraw.palette import MANUAL, WHITE, Tone
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import BODY_SIZE

from . import lifecycle

SLUG = "order-install-partition"
TITLE = "Steps 1 to 3: order, install, partition"

SUBTITLE = "Three steps in the OVH panel and the installer, once per machine."

CARDS_Y = 296.0
CARD_HEIGHT = 400.0
CARD_GAP = 34.0
CARD_WIDTH = (CONTENT_WIDTH - 2 * CARD_GAP) / 3.0
ICON_SIZE = 148.0

NUMBER_Y = 236.0

CLOSING_Y = 764.0
CLOSING = (
    "None of the three is in the Ansible repository. Step 4 is where the "
    "repository starts."
)

HAND_CARD = Tone(MANUAL.stroke, WHITE, MANUAL.text)

CARDS = (
    IconCard(
        icons.host,
        icons.HOST_ASPECT,
        "Order the machine",
        "the model, the options, the datacentre",
        HAND_CARD,
        MANUAL,
    ),
    IconCard(
        icons.os_install,
        icons.OS_INSTALL_ASPECT,
        "Install Debian",
        "the operating system, on the delivered machine",
        HAND_CARD,
        MANUAL,
    ),
    IconCard(
        icons.disk_layout,
        icons.DISK_LAYOUT_ASPECT,
        "Partition the disks",
        "RAID, filesystems, mount points",
        HAND_CARD,
        MANUAL,
    ),
)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, TITLE, SUBTITLE)
    icon_card_row(
        scene,
        MARGIN,
        CARDS_Y,
        CARDS,
        card_height=CARD_HEIGHT,
        icon_size=ICON_SIZE,
        gap=CARD_GAP,
        total_width=CONTENT_WIDTH,
    )
    for position in range(len(CARDS)):
        centre = MARGIN + position * (CARD_WIDTH + CARD_GAP) + CARD_WIDTH / 2.0
        scene.rectangle(
            centre - lifecycle.NUMBER_SIZE / 2.0,
            NUMBER_Y,
            lifecycle.NUMBER_SIZE,
            lifecycle.NUMBER_SIZE,
            HAND_CARD,
            label=str(position + 1),
            label_font_size=BODY_SIZE,
        )
    note(scene, MARGIN, CLOSING_Y, CLOSING, width=CONTENT_WIDTH)
    return scene
