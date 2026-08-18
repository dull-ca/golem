from __future__ import annotations

from excalidraw.layout import note
from excalidraw.palette import INK, INK_FAINT, INK_SOFT
from excalidraw.scene import CANVAS_WIDTH, CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import CAPTION_SIZE, HEADING_SIZE, TITLE_SIZE

from . import TITLE, golem_symbol

SLUG = "title"

# NOTE: the deck author's own sentence, verbatim — not to be rewritten,
# tightened, or re-punctuated. The line breaks fall after "want," and after
# "program", its two clause boundaries, so the three claims read as three
# claims; don't collapse them into one string and let the wrap fall where it
# lands.
TAGLINE = (
    "Your Debian fleet in the state you want,\n"
    "from one typed program\n"
    "with an undo for all changes"
)

SYMBOL_HEIGHT = 280.0
SYMBOL_Y = 240.0
HEADLINE_Y = 560.0
TAGLINE_Y = 645.0
CREDIT_Y = 880.0


def build() -> Scene:
    scene = Scene(SLUG)
    emblem = golem_symbol.mark()
    scene.image(
        (CANVAS_WIDTH - emblem.aspect * SYMBOL_HEIGHT) / 2.0,
        SYMBOL_Y,
        SYMBOL_HEIGHT,
        emblem,
    )
    note(
        scene,
        MARGIN,
        HEADLINE_Y,
        TITLE,
        width=CONTENT_WIDTH,
        font_size=TITLE_SIZE,
        colour=INK,
        align="center",
    )
    note(
        scene,
        MARGIN,
        TAGLINE_Y,
        TAGLINE,
        width=CONTENT_WIDTH,
        font_size=HEADING_SIZE,
        colour=INK_SOFT,
        align="center",
    )
    note(
        scene,
        MARGIN,
        CREDIT_Y,
        golem_symbol.CREDIT,
        width=CONTENT_WIDTH,
        font_size=CAPTION_SIZE,
        colour=INK_FAINT,
        align="center",
    )
    return scene
