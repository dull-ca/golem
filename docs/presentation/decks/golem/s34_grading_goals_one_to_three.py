from __future__ import annotations

from excalidraw.layout import slide_header
from excalidraw.scene import Scene

from . import goals, scorecard

SLUG = "grading-goals-1-to-3"
TITLE = "Grading the goals: 1 to 3"

ROWS = scorecard.rows_for_goals(goals.UNDOABLE, goals.PLANNABLE)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, TITLE)
    scorecard.draw(scene, ROWS)
    return scene
