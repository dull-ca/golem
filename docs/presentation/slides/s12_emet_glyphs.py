from __future__ import annotations

from typing import Sequence

from excalidraw.layout import (
    LabelledBox,
    TextLine,
    callout,
    labelled_box,
    note,
    slide_header,
    span_bar,
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
    TEAL,
    TEAL_FILL,
    VIOLET,
    VIOLET_FILL,
    Tone,
)
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.text import HAND, MONO

SLUG = "emet-and-the-glyphs"
TITLE = "Emet and the four glyphs"

SCROLL_TONE = Tone(SLATE, SLATE_FILL)
BRANCH_TONE = Tone(BLUE, BLUE_FILL)
LEAF_TONE = Tone(VIOLET, VIOLET_FILL)
GLYPH_TONE = Tone(TEAL, TEAL_FILL)

HEADER_Y = MARGIN

LEFT_X = MARGIN
LEFT_WIDTH = 448.0
RIGHT_X = 536.0
RIGHT_WIDTH = 1000.0

SIGNATURE_Y = 160.0
ROOT_Y = 252.0
ROOT_WIDTH = 200.0
ROOT_HEIGHT = 46.0
CHILD_Y = 344.0
CHILD_WIDTH = 210.0
CHILD_HEIGHT = 84.0
TREE_NOTE_Y = 452.0
FIRST_CALLOUT_Y = 552.0
SECOND_CALLOUT_Y = 675.0

GLYPHS_Y = 160.0
GLYPH_GAP = 54.0

CLOSING_Y = 800.0
CLOSING_HEIGHT = 48.0


def literal(body: str, size: float = 13) -> TextLine:
    return (body, size, MONO)


def gloss(body: str, size: float = 13) -> TextLine:
    return (body, size, HAND)


GLYPH_CARDS: tuple[Sequence[TextLine], ...] = (
    (
        literal("aptPackage { name }", 16),
        literal("Glyph::AptPackage { name }        key   apt:<name>"),
        gloss("a Debian package"),
    ),
    (
        literal("systemdService { unit }", 16),
        literal("Glyph::SystemdService { unit }        key   systemd:<unit>"),
        gloss("an enabled and started unit"),
    ),
    (
        literal(
            "file { path, contents, mode }     directory { path, mode }     "
            "symlink { path, target }",
            15,
        ),
        gloss("three surface spellings of one glyph — the count stays four"),
        literal("Glyph::Filesystem { path, entry: Entry }        key   file:<path>"),
        literal("Entry = File { contents, perms } | Directory { perms } | Symlink { target }"),
        literal("Perms { mode: u16, owner: Option<String>, group: Option<String> }"),
        gloss("each arm carries only its own fields, so a symlink with a mode cannot be written"),
    ),
    (
        literal("lineInFile { path, line }", 16),
        literal("Glyph::LineInFile { path, line }        key   fileline:<path>:<line>"),
        gloss("one line ensured present in a file"),
    ),
)


def draw_scroll_tree(scene: Scene) -> None:
    text_card(
        scene,
        LEFT_X,
        SIGNATURE_Y,
        LEFT_WIDTH,
        (
            literal("main : List Scroll", 16),
            gloss("one Scroll per host, one program for the fleet"),
        ),
        SCROLL_TONE,
    )
    root_x = LEFT_X + (LEFT_WIDTH - ROOT_WIDTH) / 2.0
    root = scene.rectangle(
        root_x,
        ROOT_Y,
        ROOT_WIDTH,
        ROOT_HEIGHT,
        SCROLL_TONE,
        label="Scroll",
        label_font_size=18,
    )
    branch = labelled_box(
        scene,
        LEFT_X,
        CHILD_Y,
        CHILD_WIDTH,
        CHILD_HEIGHT,
        LabelledBox("branch", "named sub-scrolls", BRANCH_TONE),
        title_font_size=18,
        detail_font_size=13,
        align="center",
    )
    leaf = labelled_box(
        scene,
        LEFT_X + LEFT_WIDTH - CHILD_WIDTH,
        CHILD_Y,
        CHILD_WIDTH,
        CHILD_HEIGHT,
        LabelledBox("leaf unit", "glyphs, and an optional policy", LEAF_TONE),
        title_font_size=18,
        detail_font_size=13,
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
        LEFT_X,
        TREE_NOTE_Y,
        "Each level holds either glyphs or named sub-scrolls — never both. A leaf unit "
        "is the failure-isolation boundary: one unit's failure never rolls back a sibling.",
        width=LEFT_WIDTH,
    )
    callout(
        scene,
        LEFT_X,
        FIRST_CALLOUT_Y,
        LEFT_WIDTH,
        "Richer shapes — workloads, quadlets, ingress — are Emet library abstractions "
        "that compile down to these four. golemd never grows a fifth kind.",
        tone=GOLEM,
    )
    callout(
        scene,
        LEFT_X,
        SECOND_CALLOUT_Y,
        LEFT_WIDTH,
        "The four require systemd. They do not assume quadlets — an older or different "
        "machine can be given another approach.",
        tone=NEUTRAL,
    )


def draw_glyph_cards(scene: Scene) -> None:
    cursor = GLYPHS_Y
    for lines in GLYPH_CARDS:
        drawn = text_card(scene, RIGHT_X, cursor, RIGHT_WIDTH, lines, GLYPH_TONE)
        cursor = drawn["y"] + drawn["height"] + GLYPH_GAP


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        "Emet and the four glyphs",
        "A typed, functional program evaluates to one Scroll per host — and everything "
        "in it comes over exactly four glyphs.",
        y=HEADER_Y,
    )
    draw_scroll_tree(scene)
    draw_glyph_cards(scene)
    span_bar(
        scene,
        MARGIN,
        CLOSING_Y,
        CONTENT_WIDTH,
        "Four glyph kinds is the whole contract between the language and the daemon.",
        tone=GOLEM,
        height=CLOSING_HEIGHT,
        font_size=17,
    )
    return scene
