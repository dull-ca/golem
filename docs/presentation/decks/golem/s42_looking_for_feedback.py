from __future__ import annotations

from excalidraw.layout import slide_header
from excalidraw.palette import INK, INK_SOFT
from excalidraw.scene import CANVAS_HEIGHT, MARGIN, Scene
from excalidraw.text import LINE_HEIGHT, measured_width
from excalidraw.type_scale import HEADING_SIZE

SLUG = "looking-for-feedback"
TITLE = "Looking for feedback"

# NOTE: this is the slide that stays on screen while the room talks, so
# nothing on it may need explaining: no figure, no legend, no subtitle.

# NOTE: question 3 is the longest, and it caps this size: at 44pt it runs past
# the right canvas margin and at TITLE_SIZE it is far past. See SPEC.md,
# "38 · Looking for feedback", for the measurements. Rewording question 3
# means re-measuring rather than assuming 40 still fits.
QUESTION_SIZE = HEADING_SIZE + 10.0
QUESTION_RHYTHM = 132.0

NUMBER_X = MARGIN + 16.0
NUMBER_WIDTH = 80.0
QUESTION_X = NUMBER_X + NUMBER_WIDTH

LIST_TOP = 200.0
LIST_BOTTOM = CANVAS_HEIGHT - MARGIN

# NOTE: Dr. Dub's own words, verbatim — not to be rewritten or tightened.
QUESTIONS: tuple[str, ...] = (
    "Worth pursuing?",
    "Ideas you'd like to see added?",
    "Can I start using it to manage some boxes, like the irwin stack?",
    "Will you help, and learn Emet?",
)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, TITLE)
    block = QUESTION_RHYTHM * (len(QUESTIONS) - 1) + QUESTION_SIZE * LINE_HEIGHT
    cursor = LIST_TOP + (LIST_BOTTOM - LIST_TOP - block) / 2.0
    for position, question in enumerate(QUESTIONS, start=1):
        scene.text(
            NUMBER_X,
            cursor,
            f"{position}.",
            font_size=QUESTION_SIZE,
            colour=INK_SOFT,
            width=NUMBER_WIDTH,
        )
        scene.text(
            QUESTION_X,
            cursor,
            question,
            font_size=QUESTION_SIZE,
            colour=INK,
            width=measured_width(question, QUESTION_SIZE),
        )
        cursor += QUESTION_RHYTHM
    return scene
