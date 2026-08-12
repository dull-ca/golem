"""The four steps are Dr. Dub's account of the December procedure.

The contrast this slide draws is expressibility, not choreography. golem turns
the sequence into one edit and one apply, because both sides fall out of the
same manifest being diffed per host. It does not order them: `golemctl fleet`
spawns one task per target with no barrier between them
(`apps/golemctl/src/fleet.rs`), and no ADR or TODO proposes otherwise. The
closing note says so, and must keep saying so — a drawn cutover would be a
feature the code does not have.
"""

from __future__ import annotations

from excalidraw.layout import LabelledBox, box_stack, note, slide_header, span_bar
from excalidraw.palette import GAP, GOLEM, MANUAL
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import BODY_SIZE, HEADING_SIZE

SLUG = "december-moving-a-service"
TITLE = "December: moving a service"

STEPS_Y = 190.0
STEP_HEIGHT = 118.0
STEP_GAP = 16.0

FAILURE_NOTE_Y = 724.0
GOLEM_BAR_Y = 772.0
GOLEM_BAR_HEIGHT = 62.0
LIMIT_NOTE_Y = 858.0

STEPS = (
    LabelledBox(
        "Edit the definition", "mark the service disabled", MANUAL, index_label="1"
    ),
    LabelledBox("Apply to host A", "it stops and uninstalls", MANUAL, index_label="2"),
    LabelledBox(
        "Edit again", "remove from host A, add to host B", MANUAL, index_label="3"
    ),
    LabelledBox("Apply", "it installs on host B", MANUAL, index_label="4"),
)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, TITLE)
    box_stack(
        scene,
        MARGIN,
        STEPS_Y,
        CONTENT_WIDTH,
        STEPS,
        box_height=STEP_HEIGHT,
        gap=STEP_GAP,
        title_font_size=HEADING_SIZE,
        detail_font_size=BODY_SIZE,
    )
    note(
        scene,
        MARGIN,
        FAILURE_NOTE_Y,
        "Out of order, it runs on both hosts or on neither.",
        width=CONTENT_WIDTH,
    )
    span_bar(
        scene,
        MARGIN,
        GOLEM_BAR_Y,
        CONTENT_WIDTH,
        "In golem: one edit, one apply. B installs it, A removes it.",
        tone=GOLEM,
        height=GOLEM_BAR_HEIGHT,
    )
    note(
        scene,
        MARGIN,
        LIMIT_NOTE_Y,
        "Nothing orders the two, so both or neither may be running briefly.",
        width=CONTENT_WIDTH,
        colour=GAP.stroke,
    )
    return scene
