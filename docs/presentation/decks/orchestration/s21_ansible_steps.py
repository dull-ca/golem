from __future__ import annotations

from excalidraw.layout import LabelledBox, callout, connector, labelled_box, slide_header
from excalidraw.palette import ANSIBLE, GAP, INK_SOFT, NEUTRAL, WHITE, Tone
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.text import MONO
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE

from .. import machines

SLUG = "ansible-steps"
TITLE = "Ansible: a play is an ordered list of steps"

SUBTITLE = "Each step is a command that changes the host. The order is the program."

PLAY_X = MARGIN
PLAY_Y = 200.0
PLAY_WIDTH = 620.0
PLAY_HEADER = 58.0
STEP_HEIGHT = 76.0
STEP_GAP = 12.0
STEP_INSET = 18.0

HOST_X = 1010.0
HOST_Y = 300.0
HOST_WIDTH = 460.0
HOST_HEIGHT = 196.0

CALLOUT_Y = 690.0

STEPS = (
    "apt: name=podman",
    "template: lila.container",
    "lineinfile: /etc/hosts",
    "systemd: state=restarted",
)

IDEMPOTENCE = (
    "Each step has to be written so that running it twice is safe. "
    "Nothing in Ansible checks that it is."
)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, TITLE, SUBTITLE)
    play_height = (
        PLAY_HEADER + len(STEPS) * STEP_HEIGHT + (len(STEPS) - 1) * STEP_GAP + STEP_INSET
    )
    scene.rectangle(PLAY_X, PLAY_Y, PLAY_WIDTH, play_height, NEUTRAL)
    scene.text(
        PLAY_X + STEP_INSET,
        PLAY_Y + 16.0,
        "site.yml",
        font_size=BODY_SIZE,
        colour=INK_SOFT,
        font_family=MONO,
        width=PLAY_WIDTH - 2 * STEP_INSET,
    )
    for position, step in enumerate(STEPS):
        top = PLAY_Y + PLAY_HEADER + position * (STEP_HEIGHT + STEP_GAP)
        scene.rectangle(
            PLAY_X + STEP_INSET,
            top,
            PLAY_WIDTH - 2 * STEP_INSET,
            STEP_HEIGHT,
            Tone(ANSIBLE.stroke, WHITE),
        )
        scene.text(
            PLAY_X + STEP_INSET + 16.0,
            top + (STEP_HEIGHT - BODY_SIZE * 1.25) / 2.0,
            str(position + 1),
            font_size=BODY_SIZE,
            colour=ANSIBLE.stroke,
            width=34.0,
        )
        scene.text(
            PLAY_X + STEP_INSET + 60.0,
            top + (STEP_HEIGHT - BODY_SIZE * 1.25) / 2.0,
            step,
            font_size=BODY_SIZE,
            colour=INK_SOFT,
            font_family=MONO,
            width=PLAY_WIDTH - 2 * STEP_INSET - 76.0,
        )
    connector(
        scene,
        [
            (PLAY_X + PLAY_WIDTH + 20.0, HOST_Y + HOST_HEIGHT / 2.0),
            (HOST_X - 20.0, HOST_Y + HOST_HEIGHT / 2.0),
        ],
        stroke=ANSIBLE.stroke,
        stroke_width=3,
        label="run top to bottom, on the host",
    )
    machines.draw_machine(
        scene,
        HOST_X,
        HOST_Y,
        machines.Machine(
            "orbit", tool_units=len(STEPS), keeper=ANSIBLE, unit_tone=ANSIBLE
        ),
        width=HOST_WIDTH,
        height=HOST_HEIGHT,
        name_font_size=BODY_SIZE,
    )
    labelled_box(
        scene,
        HOST_X,
        HOST_Y + HOST_HEIGHT + 22.0,
        HOST_WIDTH,
        104.0,
        LabelledBox(
            "The host is what the steps left behind",
            "nothing recorded what it was before",
            NEUTRAL,
        ),
        title_font_size=CAPTION_SIZE,
        detail_font_size=CAPTION_SIZE,
        align="center",
    )
    callout(
        scene,
        MARGIN,
        CALLOUT_Y,
        CONTENT_WIDTH,
        IDEMPOTENCE,
        tone=GAP,
        font_size=BODY_SIZE,
    )
    return scene
