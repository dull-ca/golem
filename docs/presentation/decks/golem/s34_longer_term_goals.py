"""The right panel draws both halves of the subtitle against checkable limits.

Templating is the multi-line string literal Emet does not have (a raw newline
before the closing quote is an "unterminated string literal",
`apps/emet/src/lexer.rs:438-443`), which is why every config file in `lib/` is a
`String.join "\\n"`. Typed configuration is the rendered text a `file` glyph
carries: `infer.rs:1409-1445` unifies every glyph field with `String`, and `emet`
links no YAML, nftables or unit-file parser. Neither box asks for a fifth glyph
kind -- `Routing.Route`, `Traefik.Ingress` and `Quadlet.Expose` are Emet types
today, and they bottom out in `file` contents.
"""

from __future__ import annotations

from excalidraw.layout import (
    PANEL_PADDING,
    Area,
    LabelledBox,
    badge,
    labelled_box,
    labelled_box_height_for,
    note,
    panel,
    panel_height_for,
    slide_header,
)
from excalidraw.palette import GOLEM, INK_GHOST, INK_SOFT, WHITE, Tone
from excalidraw.scene import CONTENT_WIDTH, LABEL_HEADROOM, MARGIN, Scene, right_edge
from excalidraw.text import MONO, measured_height, wrapped
from excalidraw.type_scale import BODY_SIZE, HEADING_SIZE

SLUG = "longer-term-goals"
TITLE = "The Emet language today, and its limits"
SUBTITLE = "I want better templating, and configuration shapes as first-class types."

PANELS_Y = 230.0
PANEL_GUTTER = 76.0
PANEL_WIDTH = (CONTENT_WIDTH - PANEL_GUTTER) / 2.0
BODY_WIDTH = PANEL_WIDTH - 2 * PANEL_PADDING

CHIP_FONT_SIZE = HEADING_SIZE
CHIP_RHYTHM = 92.0
CHIP_GAP = 14.0
CHIP_HEIGHT = CHIP_FONT_SIZE * 1.25 + 16.0
WANTED_GAP = 30.0

ABSENT = Tone(INK_GHOST, WHITE, INK_SOFT)
ABSENT_HEADING = Tone(INK_SOFT, WHITE, INK_SOFT)

TODAY_HEADING = "What a program can say today"
WANTED_HEADING = "What the language does not do yet"

GLYPH_SPELLING_ROWS: tuple[tuple[str, ...], ...] = (
    ("aptPackage",),
    ("systemdService",),
    ("file", "directory", "symlink"),
    ("lineInFile",),
)

TODAY_CAPTION = "Routes, port exposure and quadlet units are already Emet types."

WANTED = (
    LabelledBox(
        "A multi-line string literal",
        "a config file is written as one-line strings joined with a newline",
        ABSENT,
    ),
    LabelledBox(
        "The shape of a rendered config file",
        "a Traefik config or an nftables rule is file contents, and nothing "
        "type-checks the rendered text",
        ABSENT,
    ),
    LabelledBox(
        "File ownership",
        "the wire model carries an owner and a group; the language does not set them",
        ABSENT,
    ),
)


WANTED_HEIGHTS = tuple(
    labelled_box_height_for(
        box, BODY_WIDTH, title_font_size=HEADING_SIZE, detail_font_size=BODY_SIZE
    )
    for box in WANTED
)


def today_height() -> float:
    caption = wrapped(TODAY_CAPTION, BODY_WIDTH * LABEL_HEADROOM, BODY_SIZE)
    chips = len(GLYPH_SPELLING_ROWS) * CHIP_RHYTHM + 12.0
    return chips + measured_height(caption, BODY_SIZE)


def wanted_height() -> float:
    return sum(WANTED_HEIGHTS) + WANTED_GAP * (len(WANTED) - 1)


def panels_height() -> float:
    return max(
        panel_height_for(TODAY_HEADING, PANEL_WIDTH, today_height()),
        panel_height_for(WANTED_HEADING, PANEL_WIDTH, wanted_height()),
    )


def draw_spellings(scene: Scene, area: Area) -> None:
    for position, row in enumerate(GLYPH_SPELLING_ROWS):
        cursor = area.x
        for spelling in row:
            chip = badge(
                scene,
                cursor,
                area.y + position * CHIP_RHYTHM,
                spelling,
                tone=Tone(GOLEM.stroke, WHITE, GOLEM.stroke),
                font_size=CHIP_FONT_SIZE,
                height=CHIP_HEIGHT,
                font_family=MONO,
            )
            cursor = right_edge(chip) + CHIP_GAP
    note(
        scene,
        area.x,
        area.y + len(GLYPH_SPELLING_ROWS) * CHIP_RHYTHM + 12.0,
        TODAY_CAPTION,
        width=area.width,
        font_size=BODY_SIZE,
    )


def draw_wanted(scene: Scene, area: Area) -> None:
    cursor = area.y
    for box, height in zip(WANTED, WANTED_HEIGHTS):
        labelled_box(
            scene,
            area.x,
            cursor,
            area.width,
            height,
            box,
            title_font_size=HEADING_SIZE,
            detail_font_size=BODY_SIZE,
            stroke_style="dotted",
        )
        cursor += height + WANTED_GAP


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, TITLE, SUBTITLE)
    height = panels_height()
    today = panel(
        scene, MARGIN, PANELS_Y, PANEL_WIDTH, height, TODAY_HEADING, tone=GOLEM
    )
    wanted = panel(
        scene,
        MARGIN + PANEL_WIDTH + PANEL_GUTTER,
        PANELS_Y,
        PANEL_WIDTH,
        height,
        WANTED_HEADING,
        tone=ABSENT_HEADING,
        stroke_style="dotted",
    )
    draw_spellings(scene, today.body)
    draw_wanted(scene, wanted.body)
    return scene
