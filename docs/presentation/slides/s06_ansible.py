"""The same figure again, coloured by what an Ansible playbook could actually reach.

Against slide 05's single platform colour this reads as absence: 1, 2 and 4 are
covered, 3 is mostly manual, and 5 and 6 are manual outright — every orchestration
part left to a human. Nothing is redrawn; only the tones and tags differ.
"""

from __future__ import annotations

from excalidraw.layout import legend, note, slide_header
from excalidraw.palette import ANSIBLE, MANUAL
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene

from . import lichess_stack

SLUG = "ansible"
TITLE = "Where we were: Ansible"

FIGURE_Y = 196
FIGURE_HEIGHT = 552

ANSIBLE_TAG = "Ansible"

LAYER_TONES = {
    1: ANSIBLE,
    2: ANSIBLE,
    3: MANUAL,
    4: ANSIBLE,
    5: MANUAL,
    6: MANUAL,
}

LAYER_TAGS = {
    1: ANSIBLE_TAG,
    2: ANSIBLE_TAG,
    3: "mostly manual",
    4: ANSIBLE_TAG,
    5: "manual",
    6: "humans deciding, humans doing",
}

PART_TONES = {part.number: MANUAL for part in lichess_stack.ORCHESTRATION_PARTS}


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        "Where we were: Ansible",
        "The same six layers, coloured by what a playbook could actually reach.",
    )
    figure = lichess_stack.draw(
        scene,
        y=FIGURE_Y,
        height=FIGURE_HEIGHT,
        layer_tones=LAYER_TONES,
        layer_tags=LAYER_TAGS,
        part_tones=PART_TONES,
        show_details=False,
    )
    legend(
        scene,
        MARGIN,
        figure.bottom + 28,
        (
            (ANSIBLE, "Ansible covered it"),
            (MANUAL, "manual — a human decided and a human did it"),
        ),
    )
    note(
        scene,
        MARGIN,
        figure.bottom + 64,
        "Layer 3 was mostly manual: DNS entries, proxy configuration and load balancer "
        "members were edited by hand. Layers 5 and 6 were manual outright — a human "
        "chose the host, a human ran the change, and all five parts of orchestration "
        "lived in someone's head.",
        width=CONTENT_WIDTH,
    )
    return scene
