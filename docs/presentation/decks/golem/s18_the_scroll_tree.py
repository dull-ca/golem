from __future__ import annotations

from excalidraw.layout import (
    LabelledBox,
    TextLine,
    callout,
    labelled_box,
    note,
    slide_header,
    text_card,
)
from excalidraw.palette import (
    BLUE,
    BLUE_FILL,
    GOLEM,
    INK_FAINT,
    NEUTRAL,
    SLATE,
    SLATE_FILL,
    VIOLET,
    VIOLET_FILL,
    Tone,
)
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.text import HAND, MONO
from excalidraw.type_scale import BODY_SIZE, HEADING_SIZE

SLUG = "the-scroll-tree"
TITLE = "One program, one scroll per host"

SCROLL_TONE = Tone(SLATE, SLATE_FILL)
BRANCH_TONE = Tone(BLUE, BLUE_FILL)
LEAF_TONE = Tone(VIOLET, VIOLET_FILL)

SIGNATURE_WIDTH = 640.0
SIGNATURE_X = MARGIN + (CONTENT_WIDTH - SIGNATURE_WIDTH) / 2.0
SIGNATURE_Y = 190.0

ROOT_WIDTH = 240.0
ROOT_HEIGHT = 70.0
ROOT_X = MARGIN + (CONTENT_WIDTH - ROOT_WIDTH) / 2.0
ROOT_Y = 330.0

CHILD_WIDTH = 460.0
CHILD_HEIGHT = 130.0
CHILD_Y = 460.0
BRANCH_X = MARGIN + 160.0
LEAF_X = MARGIN + CONTENT_WIDTH - CHILD_WIDTH - 160.0

NOTE_Y = 632.0
CALLOUT_Y = 716.0
CALLOUT_GAP = 40.0
CALLOUT_WIDTH = (CONTENT_WIDTH - CALLOUT_GAP) / 2.0


def literal(body: str, size: float = BODY_SIZE) -> TextLine:
    return (body, size, MONO)


def gloss(body: str, size: float = BODY_SIZE) -> TextLine:
    return (body, size, HAND)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        TITLE,
        "An Emet program is typed and functional, and evaluates to a list of scrolls.",
    )
    text_card(
        scene,
        SIGNATURE_X,
        SIGNATURE_Y,
        SIGNATURE_WIDTH,
        (literal("main : List Scroll"), gloss("one Scroll per host")),
        SCROLL_TONE,
        align="center",
    )
    root = scene.rectangle(
        ROOT_X,
        ROOT_Y,
        ROOT_WIDTH,
        ROOT_HEIGHT,
        SCROLL_TONE,
        label="Scroll",
        label_font_size=HEADING_SIZE,
    )
    branch = labelled_box(
        scene,
        BRANCH_X,
        CHILD_Y,
        CHILD_WIDTH,
        CHILD_HEIGHT,
        LabelledBox("branch", "named sub-scrolls", BRANCH_TONE),
        title_font_size=HEADING_SIZE,
        detail_font_size=BODY_SIZE,
        align="center",
    )
    leaf = labelled_box(
        scene,
        LEAF_X,
        CHILD_Y,
        CHILD_WIDTH,
        CHILD_HEIGHT,
        LabelledBox("leaf unit", "glyphs, and an optional policy", LEAF_TONE),
        title_font_size=HEADING_SIZE,
        detail_font_size=BODY_SIZE,
        align="center",
    )
    fork_x = root["x"] + ROOT_WIDTH / 2.0
    fork_y = ROOT_Y + ROOT_HEIGHT
    for child in (branch, leaf):
        scene.line(
            [(fork_x, fork_y), (child["x"] + CHILD_WIDTH / 2.0, CHILD_Y)],
            stroke=INK_FAINT,
        )
    note(
        scene,
        MARGIN,
        NOTE_Y,
        "Either glyphs or named sub-scrolls at each level — never both.",
        width=CONTENT_WIDTH,
    )
    callout(
        scene,
        MARGIN,
        CALLOUT_Y,
        CALLOUT_WIDTH,
        "A leaf unit is the failure-isolation boundary: one unit's failure never rolls "
        "back a sibling.",
        tone=NEUTRAL,
    )
    callout(
        scene,
        MARGIN + CALLOUT_WIDTH + CALLOUT_GAP,
        CALLOUT_Y,
        CALLOUT_WIDTH,
        "Workloads, quadlets and ingress are Emet libraries that compile down to the "
        "four glyphs.",
        tone=GOLEM,
    )
    return scene
