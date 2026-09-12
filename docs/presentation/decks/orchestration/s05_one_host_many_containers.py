from __future__ import annotations

from excalidraw import icons
from excalidraw.layout import LabelledBox, badge, box_row, slide_header, span_bar
from excalidraw.palette import (
    CONTAINER,
    NEUTRAL,
    NODE,
    SLATE,
    TEAL,
    WHITE,
    WORKLOAD,
    Tone,
)
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import CAPTION_SIZE

SLUG = "one-host-many-containers"
TITLE = "One host, many containers"

BAND_Y = 190.0
BAND_HEIGHT = 270.0

HOST_X = 112.0
HOST_Y = 214.0
HOST_SIZE = 190.0
HOST_BADGE_Y = 412.0

CONTAINER_Y = 250.0
CONTAINER_SIZE = 100.0
CONTAINER_XS = (500.0, 690.0, 880.0, 1070.0, 1260.0)
CONTAINER_BADGE_X = 945.0
CONTAINER_BADGE_Y = 412.0

JOBS_Y = 500.0
JOB_WIDTH = 275.2
JOB_HEIGHT = 160.0
JOB_GAP = 24.0

BAR_Y = 720.0
BAR_HEIGHT = 64.0

JOB_TONE = Tone(TEAL, WHITE)

JOBS = (
    LabelledBox("Pull", "images by digest", JOB_TONE),
    LabelledBox("Start and stop", "and restart", JOB_TONE),
    LabelledBox("Network", "ports and bridges", JOB_TONE),
    LabelledBox("Volumes", "host paths mounted in", JOB_TONE),
    LabelledBox("Policy", "restart on failure", JOB_TONE),
)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        "One host, many containers",
        "A container runtime is the program that runs containers on one host.",
    )
    scene.rectangle(MARGIN, BAND_Y, CONTENT_WIDTH, BAND_HEIGHT, NEUTRAL)
    icons.host(scene, HOST_X, HOST_Y, HOST_SIZE, tone=NODE)
    badge(
        scene,
        HOST_X,
        HOST_BADGE_Y,
        "one host",
        tone=Tone(SLATE, WHITE),
        font_size=CAPTION_SIZE,
    )
    for left in CONTAINER_XS:
        icons.container(scene, left, CONTAINER_Y, CONTAINER_SIZE, tone=WORKLOAD)
    badge(
        scene,
        CONTAINER_BADGE_X,
        CONTAINER_BADGE_Y,
        "many containers",
        tone=JOB_TONE,
        font_size=CAPTION_SIZE,
        anchor="center",
    )
    box_row(
        scene,
        MARGIN,
        JOBS_Y,
        JOBS,
        box_width=JOB_WIDTH,
        box_height=JOB_HEIGHT,
        gap=JOB_GAP,
    )
    span_bar(
        scene,
        MARGIN,
        BAR_Y,
        CONTENT_WIDTH,
        "The runtime does all of this on one machine only.",
        tone=CONTAINER,
        height=BAR_HEIGHT,
    )
    return scene
