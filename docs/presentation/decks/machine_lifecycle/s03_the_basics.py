from __future__ import annotations

from excalidraw.layout import callout, connector, note, slide_header
from excalidraw.palette import ANSIBLE, GAP, INK_SOFT
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.text import HAND
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE

from .. import machines
from ..ansible_play import IDEMPOTENCE, draw_play, play_height

SLUG = "the-basics"
TITLE = "Step 4: Ansible installs the basics"

SUBTITLE = "Ansible is a controller that runs an ordered list of steps against many machines over ssh."

PLAY_X = MARGIN
PLAY_Y = 200.0
PLAY_WIDTH = 560.0
PLAY_STEP_HEIGHT = 62.0

BASICS = (
    "the firewall",
    "the ssh rules",
    "the vrack",
    "the hostnames",
    "ntp",
    "the tools",
)

HOST_X = 1000.0
HOST_WIDTH = 420.0
HOST_HEIGHT = 176.0

HOST_STATE = "the core OS, the network and the security rules; no services yet"

CALLOUT_Y = 748.0


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, TITLE, SUBTITLE)
    draw_play(
        scene,
        PLAY_X,
        PLAY_Y,
        PLAY_WIDTH,
        BASICS,
        filename="site.yml",
        step_font_family=HAND,
        step_height=PLAY_STEP_HEIGHT,
    )
    host_y = PLAY_Y + (play_height(len(BASICS), PLAY_STEP_HEIGHT) - HOST_HEIGHT) / 2.0
    connector(
        scene,
        [
            (PLAY_X + PLAY_WIDTH + 20.0, host_y + HOST_HEIGHT / 2.0),
            (HOST_X - 20.0, host_y + HOST_HEIGHT / 2.0),
        ],
        stroke=ANSIBLE.stroke,
        stroke_width=3,
        label="the same play, on every machine",
    )
    machines.draw_machine(
        scene,
        HOST_X,
        host_y,
        machines.Machine("orbit", keeper=ANSIBLE),
        width=HOST_WIDTH,
        height=HOST_HEIGHT,
        name_font_size=BODY_SIZE,
    )
    note(
        scene,
        HOST_X,
        host_y + HOST_HEIGHT + 16.0,
        HOST_STATE,
        width=HOST_WIDTH,
        font_size=CAPTION_SIZE,
        colour=INK_SOFT,
        align="center",
    )
    callout(scene, MARGIN, CALLOUT_Y, CONTENT_WIDTH, IDEMPOTENCE, tone=GAP)
    return scene
