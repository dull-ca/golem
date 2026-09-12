from __future__ import annotations

from excalidraw.layout import note, slide_header
from excalidraw.palette import ANSIBLE, GOLEM, MANUAL, THEIRS, WHITE, Tone
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene

from . import stack
from ..vocabulary import ORCHESTRATION_PARTS, PLACEMENT, SCALING

SLUG = "where-golem-sits"
TITLE = "Where golem sits"

SUBTITLE = "Nothing in golem chooses a node, and nothing in it moves a replica count."

GOLEM_BANDS = (5, 6, 7)
CLOSING_Y = 878.0


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, TITLE, SUBTITLE)
    stack.draw(
        scene,
        band_tones={
            **{number: THEIRS for number in stack.BOUGHT_BANDS},
            4: ANSIBLE,
            **{number: GOLEM for number in GOLEM_BANDS},
        },
        column_tone=GOLEM,
        part_tones={
            **{part.number: Tone(GOLEM.stroke, WHITE) for part in ORCHESTRATION_PARTS},
            PLACEMENT: MANUAL,
            SCALING: MANUAL,
        },
        part_stroke_styles={PLACEMENT: "dashed", SCALING: "dashed"},
    )
    stack.gutter_bar(
        scene,
        (0, stack.LANES - 1),
        (min(GOLEM_BANDS), max(GOLEM_BANDS)),
        "golem",
        GOLEM,
        detail="you write the state a host should be in, and every change it makes "
        "records how to undo itself",
    )
    stack.gutter_bar(
        scene, (0, stack.LANES - 1), (4, 4), "Ansible, as before", ANSIBLE
    )
    stack.gutter_bar(
        scene,
        (0, stack.LANES - 1),
        (min(stack.BOUGHT_BANDS), max(stack.BOUGHT_BANDS)),
        "Bought",
        THEIRS,
        detail="golem replaces none of this",
    )
    note(
        scene,
        MARGIN,
        CLOSING_Y,
        "Placement and scaling stay decisions a person makes, written down and versioned.",
        width=CONTENT_WIDTH,
    )
    return scene
