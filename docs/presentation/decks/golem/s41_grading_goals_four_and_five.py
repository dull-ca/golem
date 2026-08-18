from __future__ import annotations

from excalidraw.layout import slide_header
from excalidraw.scene import Scene

from . import goals, scorecard

SLUG = "grading-goals-4-and-5"
TITLE = "Grading the goals: 4 and 5"

ROWS = scorecard.rows_for_goals(goals.ROLLBACK, goals.NO_YAML)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, TITLE)
    scorecard.draw(scene, ROWS)
    return scene
