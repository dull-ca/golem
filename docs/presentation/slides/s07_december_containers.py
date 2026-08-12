"""The figure a third time: December's answer, and the holes left in it.

Where 06 shows one tool and 05 shows one platform, this shows four tools sharing
the layers — quadlets, custom Python, systemd, Ansible — which is why layer 6 is
tagged as having no single owner. The figure is narrowed to leave a right-hand
gutter for the gap marks, the only content unique to this slide: draining, moving
a service to another host, and rollback stayed unsolved.
"""

from __future__ import annotations

from excalidraw.layout import badge, connector, legend, note, slide_header
from excalidraw.palette import ANSIBLE, BESPOKE, GAP, MANUAL, RED, SYSTEMD
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene, centre, right_edge

from . import lichess_stack

SLUG = "december-containers"
TITLE = "December: containers"

FIGURE_Y = 196
FIGURE_WIDTH = 1200
FIGURE_HEIGHT = 552

QUADLET_TAG = "quadlets"
PYTHON_TAG = "custom Python"
ANSIBLE_TAG = "Ansible"

LAYER_TONES = {
    1: ANSIBLE,
    2: ANSIBLE,
    3: BESPOKE,
    4: SYSTEMD,
    5: SYSTEMD,
    6: MANUAL,
}

LAYER_TAGS = {
    1: ANSIBLE_TAG,
    2: ANSIBLE_TAG,
    3: PYTHON_TAG,
    4: QUADLET_TAG,
    5: QUADLET_TAG,
    6: "no single owner",
}

PLACEMENT = 1
LIFECYCLE = 2
HEALTH = 3
PLUMBING = 4
SCALING = 5

PART_TONES = {
    PLACEMENT: BESPOKE,
    LIFECYCLE: SYSTEMD,
    HEALTH: MANUAL,
    PLUMBING: BESPOKE,
    SCALING: BESPOKE,
}

PART_TAGS = {
    PLACEMENT: "python",
    LIFECYCLE: "systemd",
    HEALTH: "manual",
    PLUMBING: "python",
    SCALING: "python",
}

GUTTER_X = 1300
GAP_FONT_SIZE = 12
GAP_MARKS = (
    (PLACEMENT, 0, "no move to another host"),
    (LIFECYCLE, -17, "no drain"),
    (LIFECYCLE, 17, "no rollback"),
)


def gap_mark(scene: Scene, part: dict, offset: float, body: str) -> None:
    middle = centre(part)[1] + offset
    chip = badge(
        scene,
        GUTTER_X,
        middle - (GAP_FONT_SIZE * 1.25 + 12) / 2.0,
        body,
        tone=GAP,
        font_size=GAP_FONT_SIZE,
    )
    connector(
        scene,
        [(chip["x"] - 4, middle), (right_edge(part) + 4, middle)],
        stroke=RED,
        dashed=True,
    )


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        "December: the push to containers",
        "Quadlets, generated configuration, and the parts that stayed unsolved.",
    )
    figure = lichess_stack.draw(
        scene,
        y=FIGURE_Y,
        width=FIGURE_WIDTH,
        height=FIGURE_HEIGHT,
        layer_tones=LAYER_TONES,
        layer_tags=LAYER_TAGS,
        part_tones=PART_TONES,
        part_tags=PART_TAGS,
        show_details=False,
    )
    scene.text(
        GUTTER_X,
        figure.part(PLACEMENT)["y"] - 34,
        "still unsolved",
        font_size=15,
        colour=RED,
    )
    for part_number, offset, body in GAP_MARKS:
        gap_mark(scene, figure.part(part_number), offset, body)
    legend(
        scene,
        MARGIN,
        figure.bottom + 28,
        (
            (SYSTEMD, "quadlets — podman + systemd"),
            (BESPOKE, "custom Python + Ansible"),
            (ANSIBLE, "Ansible"),
            (MANUAL, "manual"),
            (GAP, "still unsolved"),
        ),
    )
    note(
        scene,
        MARGIN,
        figure.bottom + 64,
        "Quadlets gave lifecycle on one host. Placement and scaling were generated from "
        "a hand-maintained table, so a human still made every decision — and nothing "
        "could drain a host, move a service, or roll a change back.",
        width=CONTENT_WIDTH,
    )
    return scene
