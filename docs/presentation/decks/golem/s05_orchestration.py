from __future__ import annotations

from excalidraw.layout import LabelledBox, hub_and_satellites, note, slide_header
from excalidraw.palette import ORANGE, ORANGE_FILL, WHITE, Tone
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import BODY_SIZE

from ..vocabulary import ORCHESTRATION_PARTS

SLUG = "orchestration"
TITLE = "What orchestration means"

HUB_TONE = Tone(ORANGE, ORANGE_FILL)
PART_TONE = Tone(ORANGE, WHITE)

SATELLITES_Y = 196.0
SATELLITE_HEIGHT = 118.0
SATELLITE_GAP = 14.0
CLOSING_Y = 872.0


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        'What "orchestration" actually means',
        "Layer 6, expanded. One word that is really five separate jobs.",
    )
    hub_and_satellites(
        scene,
        MARGIN,
        SATELLITES_Y,
        CONTENT_WIDTH,
        LabelledBox("Layer 6", "one word", HUB_TONE),
        [
            LabelledBox(part.title, part.detail, PART_TONE, index_label=str(part.number))
            for part in ORCHESTRATION_PARTS
        ],
        satellite_height=SATELLITE_HEIGHT,
        gap=SATELLITE_GAP,
        satellite_detail_font_size=BODY_SIZE,
    )
    note(
        scene,
        MARGIN,
        CLOSING_Y,
        "None is optional — a platform, a script, or a human answers each.",
        width=CONTENT_WIDTH,
    )
    return scene
