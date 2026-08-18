from __future__ import annotations

from excalidraw.layout import slide_header
from excalidraw.palette import INK, INK_SOFT
from excalidraw.scene import CANVAS_HEIGHT, MARGIN, Scene
from excalidraw.text import LINE_HEIGHT, measured_width
from excalidraw.type_scale import HEADING_SIZE

from . import goals

SLUG = "the-goals"
TITLE = "What I wanted"

GOAL_SIZE = HEADING_SIZE
GOAL_RHYTHM = 108.0

NUMBER_X = MARGIN + 16.0
NUMBER_WIDTH = 72.0
STATEMENT_X = NUMBER_X + NUMBER_WIDTH

LIST_TOP = 190.0
LIST_BOTTOM = CANVAS_HEIGHT - MARGIN


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, TITLE)
    block = GOAL_RHYTHM * (len(goals.GOALS) - 1) + GOAL_SIZE * LINE_HEIGHT
    cursor = LIST_TOP + (LIST_BOTTOM - LIST_TOP - block) / 2.0
    for goal in goals.GOALS:
        scene.text(
            NUMBER_X,
            cursor,
            f"{goal.number}.",
            font_size=GOAL_SIZE,
            colour=INK_SOFT,
            width=NUMBER_WIDTH,
        )
        scene.text(
            STATEMENT_X,
            cursor,
            goal.statement,
            font_size=GOAL_SIZE,
            colour=INK,
            width=measured_width(goal.statement, GOAL_SIZE),
        )
        cursor += GOAL_RHYTHM
    return scene
