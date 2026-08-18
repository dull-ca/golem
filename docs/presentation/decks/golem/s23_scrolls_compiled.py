from __future__ import annotations

from excalidraw.layout import connector, note, slide_header, text_card
from excalidraw.palette import GOLEM, INK_FAINT, NEUTRAL, TRANSPARENT, Tone
from excalidraw.scene import MARGIN, Scene
from excalidraw.text import MONO
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE

from . import fleet
from ..lichess_fleet import TOOL_KEPT_HOSTS

SLUG = "golem-scrolls-compiled"
TITLE = "golem: emetc compiles one scroll per host"

SUBTITLE = "One source tree, one compiler, one manifest, and a scroll named for each machine."

TREE_X = MARGIN
TREE_Y = 328.0
TREE_WIDTH = 396.0

COMPILER_X = 500.0
COMPILER_Y = 456.0
COMPILER_WIDTH = 240.0
COMPILER_HEIGHT = 116.0

MANIFEST_X = 800.0
MANIFEST_Y = 274.0
MANIFEST_WIDTH = 736.0
MANIFEST_HEIGHT = 480.0
MANIFEST_PADDING = 30.0

SCROLL_WIDTH = 318.0
SCROLL_HEIGHT = 68.0
SCROLL_GAP_X = 40.0
SCROLL_GAP_Y = 22.0
SCROLL_COLUMNS = 2
SCROLL_Y = MANIFEST_Y + 96.0

NOTE_Y = 800.0

TREE_LINES = (
    ("fleet.emet", BODY_SIZE, MONO),
    ("hosts/achoo.emet", CAPTION_SIZE, MONO),
    ("hosts/apate.emet", CAPTION_SIZE, MONO),
    ("hosts/cobar.emet", CAPTION_SIZE, MONO),
    ("hosts/dingo.emet", CAPTION_SIZE, MONO),
    ("hosts/…", CAPTION_SIZE, MONO),
    ("lib/workload.emet", CAPTION_SIZE, MONO),
    ("lib/ingress.emet", CAPTION_SIZE, MONO),
)


def _scroll_origin(position: int) -> tuple[float, float]:
    return (
        MANIFEST_X + MANIFEST_PADDING
        + (position % SCROLL_COLUMNS) * (SCROLL_WIDTH + SCROLL_GAP_X),
        SCROLL_Y + (position // SCROLL_COLUMNS) * (SCROLL_HEIGHT + SCROLL_GAP_Y),
    )


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, TITLE, SUBTITLE)
    tree = text_card(scene, TREE_X, TREE_Y, TREE_WIDTH, TREE_LINES, NEUTRAL)
    note(
        scene,
        TREE_X,
        TREE_Y + tree["height"] + 14,
        "the source directory",
        width=TREE_WIDTH,
        font_size=CAPTION_SIZE,
        colour=INK_FAINT,
    )
    scene.rectangle(
        COMPILER_X,
        COMPILER_Y,
        COMPILER_WIDTH,
        COMPILER_HEIGHT,
        GOLEM,
        label="emetc build",
        label_font_size=BODY_SIZE,
        label_font_family=MONO,
    )
    middle = COMPILER_Y + COMPILER_HEIGHT / 2.0
    connector(
        scene,
        [(TREE_X + TREE_WIDTH + 12, middle), (COMPILER_X - 12, middle)],
        stroke=GOLEM.stroke,
        stroke_width=3,
    )
    connector(
        scene,
        [(COMPILER_X + COMPILER_WIDTH + 12, middle), (MANIFEST_X - 12, middle)],
        stroke=GOLEM.stroke,
        stroke_width=3,
    )
    scene.rectangle(
        MANIFEST_X,
        MANIFEST_Y,
        MANIFEST_WIDTH,
        MANIFEST_HEIGHT,
        Tone(GOLEM.stroke, TRANSPARENT),
        stroke_style="dashed",
    )
    scene.text(
        MANIFEST_X + MANIFEST_PADDING,
        MANIFEST_Y + 28,
        "one manifest",
        font_size=BODY_SIZE,
        colour=GOLEM.stroke,
        width=MANIFEST_WIDTH - 2 * MANIFEST_PADDING,
    )
    for position, host in enumerate(TOOL_KEPT_HOSTS):
        left, top = _scroll_origin(position)
        fleet.scroll_mark(scene, left, top, SCROLL_WIDTH, SCROLL_HEIGHT, host, GOLEM)
    note(
        scene,
        MANIFEST_X,
        NOTE_Y,
        "One compile for the whole fleet. Nothing is compiled per host.",
        width=MANIFEST_WIDTH,
    )
    return scene
