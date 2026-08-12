from __future__ import annotations

from excalidraw.layout import badge, note, slide_header, span_bar
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import CAPTION_SIZE

from . import lichess_ladder
from .fleet import MACHINE_OUTLINE
from .lichess_stack import DESCRIPTIVE_LAYER_TONES

SLUG = "where-lichess-sits-with-portainer"
TITLE = "Where lichess sits, with Portainer"

PORTAINER_TONE = DESCRIPTIVE_LAYER_TONES[6]

MACHINE_MARK = 44.0
MACHINE_MARK_GAP = 16.0
MACHINE_MARK_COUNT = 24
PORTAINER_MACHINE = 11

SCALE_GAP = 110.0
BADGE_GAP = 12.0
NOTE_GAP = 24.0


SCALE_WIDTH = (
    MACHINE_MARK_COUNT * MACHINE_MARK + (MACHINE_MARK_COUNT - 1) * MACHINE_MARK_GAP
)
SCALE_X = MARGIN + (CONTENT_WIDTH - SCALE_WIDTH) / 2.0


def _machine_mark_x(index: int) -> float:
    return SCALE_X + index * (MACHINE_MARK + MACHINE_MARK_GAP)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, TITLE)
    span_bar(
        scene,
        lichess_ladder.Ladder.rung_left(lichess_ladder.PORTAINER_FIRST) + 10,
        lichess_ladder.ANNOTATION_Y,
        lichess_ladder.Ladder.rung_right(lichess_ladder.PORTAINER_LAST)
        - lichess_ladder.Ladder.rung_left(lichess_ladder.PORTAINER_FIRST)
        - 20,
        "Portainer — a web UI that manages these platforms",
        tone=PORTAINER_TONE,
        height=lichess_ladder.ANNOTATION_HEIGHT,
    )
    ladder = lichess_ladder.draw(scene)
    scale_y = ladder.bottom + SCALE_GAP
    for index in range(MACHINE_MARK_COUNT):
        scene.rectangle(
            _machine_mark_x(index),
            scale_y,
            MACHINE_MARK,
            MACHINE_MARK,
            PORTAINER_TONE if index == PORTAINER_MACHINE else MACHINE_OUTLINE,
        )
    badge(
        scene,
        _machine_mark_x(PORTAINER_MACHINE) + MACHINE_MARK / 2.0,
        scale_y + MACHINE_MARK + BADGE_GAP,
        "Portainer",
        tone=PORTAINER_TONE,
        font_size=CAPTION_SIZE,
        anchor="center",
    )
    note(
        scene,
        MARGIN,
        scale_y + MACHINE_MARK + BADGE_GAP + CAPTION_SIZE * 1.25 + 16 + NOTE_GAP,
        "Roughly 20 to 30 machines, under Ansible, hand configuration and custom "
        "Python. Exactly one of them runs Portainer.",
        width=CONTENT_WIDTH,
    )
    return scene
