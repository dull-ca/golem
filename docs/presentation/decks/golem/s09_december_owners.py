from __future__ import annotations

from excalidraw.layout import LabelledBox, RhythmCard, card_rhythm, note, slide_header
from excalidraw.palette import ANSIBLE, BESPOKE, MANUAL, SYSTEMD
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import BODY_SIZE, HEADING_SIZE

SLUG = "december-owners"
TITLE = "December: who owned what"

ROWS_Y = 200.0
ROW_HEIGHT = 150.0
ROW_GAP = 24.0
CLOSING_Y = 730.0

ROWS = (
    (
        RhythmCard(1.0, LabelledBox("Ansible", "layers 1 and 2", ANSIBLE)),
        RhythmCard(1.0, LabelledBox("custom Python", "layer 3", BESPOKE)),
        RhythmCard(1.0, LabelledBox("quadlets", "layers 4 and 5", SYSTEMD)),
    ),
    (
        RhythmCard(
            1.7,
            LabelledBox(
                "custom Python + Ansible", "placement, plumbing, scaling", BESPOKE
            ),
        ),
        RhythmCard(1.0, LabelledBox("systemd", "lifecycle", SYSTEMD)),
    ),
    (
        RhythmCard(
            1.0,
            LabelledBox("Nobody", "nothing watched for drift or failure", MANUAL),
        ),
    ),
)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, "December: who owned what")
    card_rhythm(
        scene,
        MARGIN,
        ROWS_Y,
        CONTENT_WIDTH,
        ROWS,
        row_height=ROW_HEIGHT,
        row_gap=ROW_GAP,
        title_font_size=HEADING_SIZE,
        detail_font_size=BODY_SIZE,
    )
    note(
        scene,
        MARGIN,
        CLOSING_Y,
        "Layer 6 had no single owner.",
        width=CONTENT_WIDTH,
    )
    return scene
