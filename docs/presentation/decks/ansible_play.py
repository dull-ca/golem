"""A play as an ordered list of numbered steps, and the rule about running it twice.

Two decks draw it: the orchestration deck quotes four Ansible tasks to show what
a step is, and the machine-lifecycle deck lists the six things lichess installs
on a new machine. Those want different geometry — a quoted command needs the
monospace font and a taller row than a two-word topic does — so the step height
and the step font are parameters while everything else is the figure.

`IDEMPOTENCE` is here rather than on either slide because it is a claim about
Ansible, and two decks stating it in two wordings would read as two claims.
"""

from __future__ import annotations

from typing import Sequence

from excalidraw.palette import ANSIBLE, INK_SOFT, NEUTRAL, WHITE, Tone
from excalidraw.scene import Scene
from excalidraw.text import MONO
from excalidraw.type_scale import BODY_SIZE

PLAY_HEADER = 58.0
STEP_HEIGHT = 76.0
STEP_GAP = 12.0
STEP_INSET = 18.0
NUMBER_GUTTER = 60.0
NUMBER_WIDTH = 34.0

IDEMPOTENCE = (
    "Each step has to be written so that running it twice is safe. "
    "Nothing in Ansible checks that it is."
)


def play_height(steps: int, step_height: float = STEP_HEIGHT) -> float:
    return PLAY_HEADER + steps * step_height + (steps - 1) * STEP_GAP + STEP_INSET


def draw_play(
    scene: Scene,
    x: float,
    y: float,
    width: float,
    steps: Sequence[str],
    *,
    filename: str,
    step_font_family: int = MONO,
    step_font_size: float = BODY_SIZE,
    step_height: float = STEP_HEIGHT,
) -> dict:
    body = scene.rectangle(x, y, width, play_height(len(steps), step_height), NEUTRAL)
    scene.text(
        x + STEP_INSET,
        y + 16.0,
        filename,
        font_size=BODY_SIZE,
        colour=INK_SOFT,
        font_family=MONO,
        width=width - 2 * STEP_INSET,
    )
    for position, step in enumerate(steps):
        top = y + PLAY_HEADER + position * (step_height + STEP_GAP)
        scene.rectangle(
            x + STEP_INSET,
            top,
            width - 2 * STEP_INSET,
            step_height,
            Tone(ANSIBLE.stroke, WHITE),
        )
        scene.text(
            x + STEP_INSET + 16.0,
            top + (step_height - BODY_SIZE * 1.25) / 2.0,
            str(position + 1),
            font_size=BODY_SIZE,
            colour=ANSIBLE.stroke,
            width=NUMBER_WIDTH,
        )
        scene.text(
            x + STEP_INSET + NUMBER_GUTTER,
            top + (step_height - step_font_size * 1.25) / 2.0,
            step,
            font_size=step_font_size,
            colour=INK_SOFT,
            font_family=step_font_family,
            width=width - 2 * STEP_INSET - 76.0,
        )
    return body
