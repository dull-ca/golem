from __future__ import annotations

from excalidraw import icons
from excalidraw.layout import LabelledBox, badge, box_stack, slide_header
from excalidraw.palette import IMAGE, VIOLET, WHITE, Tone
from excalidraw.scene import Scene
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE, HEADING_SIZE

SLUG = "the-image"
TITLE = "The image"

MARK_X = 140.0
MARK_Y = 304.0
MARK_SIZE = 280.0
MARK_BADGE_X = 343.0
MARK_BADGE_Y = 600.0

STACK_X = 650.0
STACK_Y = 220.0
STACK_WIDTH = 886.0
BOX_HEIGHT = 150.0
BOX_GAP = 26.0

POINT_TONE = Tone(VIOLET, WHITE)

POINTS = (
    LabelledBox("Layers", "stack, and are shared between images", POINT_TONE),
    LabelledBox("Digest", "the same digest is the same bytes everywhere", POINT_TONE),
    LabelledBox("Immutable", "nothing a running container does changes it", POINT_TONE),
)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        "The image a container runs from",
        "An image is read-only filesystem layers plus a config, named by the digest "
        "of its contents.",
    )
    icons.container_image(scene, MARK_X, MARK_Y, MARK_SIZE, tone=IMAGE)
    badge(
        scene,
        MARK_BADGE_X,
        MARK_BADGE_Y,
        "one image",
        tone=POINT_TONE,
        font_size=CAPTION_SIZE,
        anchor="center",
    )
    box_stack(
        scene,
        STACK_X,
        STACK_Y,
        STACK_WIDTH,
        POINTS,
        box_height=BOX_HEIGHT,
        gap=BOX_GAP,
        title_font_size=HEADING_SIZE,
        detail_font_size=BODY_SIZE,
    )
    return scene
