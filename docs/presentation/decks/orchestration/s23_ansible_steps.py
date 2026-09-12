from __future__ import annotations

from excalidraw.layout import LabelledBox, callout, connector, labelled_box, slide_header
from excalidraw.palette import ANSIBLE, GAP, NEUTRAL
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE

from .. import machines
from ..ansible_play import IDEMPOTENCE, draw_play

SLUG = "ansible-steps"
TITLE = "Ansible: a play is an ordered list of steps"

SUBTITLE = "Each step is a command that changes the host. The order is the program."

PLAY_X = MARGIN
PLAY_Y = 200.0
PLAY_WIDTH = 620.0

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


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, TITLE, SUBTITLE)
    draw_play(scene, PLAY_X, PLAY_Y, PLAY_WIDTH, STEPS, filename="site.yml")
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
