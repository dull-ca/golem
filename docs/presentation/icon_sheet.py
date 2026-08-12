"""Every icon in the catalogue on one canvas, labelled.

A reference sheet for anyone drawing a new slide, and the reason the restore()
oracle covers marks no slide happens to use: a mark that is never emitted is
never checked against the real Excalidraw loader.
"""

from __future__ import annotations

from excalidraw import icons
from excalidraw.layout import note, slide_header
from excalidraw.palette import INK_FAINT
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import CAPTION_SIZE

ICON_SHEET_FILENAME = "icons.excalidraw"
ICON_SHEET_KEY = "icon-sheet"

COLUMNS = 5
GRID_Y = 200.0
CELL_HEIGHT = 178.0
ICON_SIZE = 84.0
LABEL_GAP = 12.0


def build_icon_sheet() -> Scene:
    scene = Scene(ICON_SHEET_KEY)
    slide_header(
        scene,
        "The icon vocabulary",
        "Drawn from rectangles, ellipses and lines. No image files, no emoji.",
    )
    cell_width = CONTENT_WIDTH / COLUMNS
    for position, spec in enumerate(icons.CATALOGUE):
        column = position % COLUMNS
        row = position // COLUMNS
        cell_x = MARGIN + column * cell_width
        cell_y = GRID_Y + row * CELL_HEIGHT
        mark_width = spec.aspect * ICON_SIZE
        spec.draw(
            scene,
            cell_x + (cell_width - mark_width) / 2.0,
            cell_y,
            ICON_SIZE,
        )
        note(
            scene,
            cell_x + 8,
            cell_y + ICON_SIZE + LABEL_GAP,
            spec.name,
            width=cell_width - 16,
            font_size=CAPTION_SIZE,
            colour=INK_FAINT,
            align="center",
        )
    return scene
