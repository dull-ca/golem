"""The review row draws the missing step absent, rather than saying it is missing.

The slot under *The software* is empty, dotted, `INK_GHOST`, reached by a dotted
arrow; the slot under *The outcome* holds `icons.person`, reached by a solid one.
No text on the slide states that the code goes unread -- the empty box beside the
full one is what carries that. Filling the empty slot with a caption instead of
leaving it empty would remove the claim, not restate it.

`icons.person` exists for this slide: the filled slot needs a subject, so the
empty slot reads as *nobody looked* rather than *not drawn yet*.
"""

from __future__ import annotations

from excalidraw import icons
from excalidraw.layout import LabelledBox, note, pipeline, slide_header
from excalidraw.palette import INK_GHOST, INK_SOFT, NEUTRAL, WHITE, Tone
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.text import LINE_HEIGHT
from excalidraw.type_scale import CAPTION_SIZE, HEADING_SIZE

SLUG = "outcome-based-development"
TITLE = "Outcome-based development"
SUBTITLE = "How golem itself was built."

CHAIN_Y = 200.0
CHAIN_HEIGHT = 160.0
CHAIN_GAP = 56.0
CHAIN_WIDTH = (CONTENT_WIDTH - 3 * CHAIN_GAP) / 4.0

REVIEW_Y = 470.0
REVIEW_HEIGHT = 330.0
REVIEW_CAPTION_Y = REVIEW_Y + REVIEW_HEIGHT + 18.0
MARK_SIZE = 190.0

ABSENT = Tone(INK_GHOST, WHITE, INK_SOFT)

CHAIN = (
    LabelledBox("Prompts"),
    LabelledBox("An LLM", "writes the software"),
    LabelledBox("The software", "golem itself"),
    LabelledBox("The outcome"),
)

CODE_COLUMN = 2
OUTCOME_COLUMN = 3

ROW_LABEL = "What a person looks at"
NO_CODE_REVIEW = "no review of the code"
OUTCOME_REVIEW = "review of the outcome"


def column_x(position: int) -> float:
    return MARGIN + position * (CHAIN_WIDTH + CHAIN_GAP)


def slot_connector(scene: Scene, position: int, *, drawn: bool) -> None:
    centre_x = column_x(position) + CHAIN_WIDTH / 2.0
    scene.arrow(
        [(centre_x, CHAIN_Y + CHAIN_HEIGHT + 8.0), (centre_x, REVIEW_Y - 8.0)],
        stroke=INK_SOFT if drawn else INK_GHOST,
        stroke_style="solid" if drawn else "dotted",
    )


def review_caption(scene: Scene, position: int, body: str) -> None:
    note(
        scene,
        column_x(position),
        REVIEW_CAPTION_Y,
        body,
        width=CHAIN_WIDTH,
        font_size=CAPTION_SIZE,
        align="center",
    )


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, TITLE, SUBTITLE)
    pipeline(
        scene,
        MARGIN,
        CHAIN_Y,
        CHAIN,
        box_width=CHAIN_WIDTH,
        box_height=CHAIN_HEIGHT,
        gap=CHAIN_GAP,
        title_font_size=HEADING_SIZE,
    )
    note(
        scene,
        MARGIN,
        REVIEW_Y + (REVIEW_HEIGHT - HEADING_SIZE * LINE_HEIGHT) / 2.0,
        ROW_LABEL,
        width=2 * CHAIN_WIDTH + CHAIN_GAP,
        font_size=HEADING_SIZE,
    )
    scene.rectangle(
        column_x(CODE_COLUMN),
        REVIEW_Y,
        CHAIN_WIDTH,
        REVIEW_HEIGHT,
        ABSENT,
        stroke_style="dotted",
    )
    slot_connector(scene, CODE_COLUMN, drawn=False)
    review_caption(scene, CODE_COLUMN, NO_CODE_REVIEW)
    scene.rectangle(
        column_x(OUTCOME_COLUMN), REVIEW_Y, CHAIN_WIDTH, REVIEW_HEIGHT, NEUTRAL
    )
    icons.person(
        scene,
        column_x(OUTCOME_COLUMN)
        + (CHAIN_WIDTH - icons.PERSON_ASPECT * MARK_SIZE) / 2.0,
        REVIEW_Y + (REVIEW_HEIGHT - MARK_SIZE) / 2.0,
        MARK_SIZE,
    )
    slot_connector(scene, OUTCOME_COLUMN, drawn=True)
    review_caption(scene, OUTCOME_COLUMN, OUTCOME_REVIEW)
    return scene
