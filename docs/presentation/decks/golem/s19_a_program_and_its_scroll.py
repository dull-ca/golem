from __future__ import annotations

from excalidraw.layout import connector, note, slide_header, text_card
from excalidraw.palette import GOLEM, INK_FAINT, NEUTRAL
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene, bottom_edge
from excalidraw.text import MONO, measured_width
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE

from . import exemplar, one_host

SLUG = "a-program-and-its-scroll"
TITLE = "A program, and the scroll it evaluates to"

SUBTITLE = (
    "An Emet program evaluates to one scroll per host, named for the host it is for."
)

CLOSING = "golemd takes the scroll that carries its own name."

CARD_WIDTH = 1180.0
CARD_X = MARGIN + (CONTENT_WIDTH - CARD_WIDTH) / 2.0

ARROW_GAP = 14.0
ARROW_RUN = 72.0
ARROW_LABEL = "emetc build"
ARROW_LABEL_GAP = 22.0

SCROLL_GAP = 16.0
NOTE_GAP = 34.0


def build() -> Scene:
    scene = Scene(SLUG)
    header_bottom = slide_header(scene, TITLE, SUBTITLE)
    one_host.check_header(header_bottom)
    card = text_card(
        scene,
        CARD_X,
        one_host.CONTENT_TOP,
        CARD_WIDTH,
        ((exemplar.PROGRAM, BODY_SIZE, MONO),),
        NEUTRAL,
    )
    arrow_top = bottom_edge(card) + ARROW_GAP
    connector(
        scene,
        [
            (one_host.SCROLL_CENTRE_X, arrow_top),
            (one_host.SCROLL_CENTRE_X, arrow_top + ARROW_RUN),
        ],
        stroke=GOLEM.stroke,
        stroke_width=3,
    )
    scene.text(
        one_host.SCROLL_CENTRE_X + ARROW_LABEL_GAP,
        arrow_top + (ARROW_RUN - CAPTION_SIZE * 1.25) / 2.0,
        ARROW_LABEL,
        font_size=CAPTION_SIZE,
        colour=INK_FAINT,
        font_family=MONO,
        width=measured_width(ARROW_LABEL, CAPTION_SIZE, MONO) + 8,
    )
    scroll_y = arrow_top + ARROW_RUN + SCROLL_GAP
    one_host.draw_scroll(scene, scroll_y)
    note(
        scene,
        MARGIN,
        scroll_y + one_host.SCROLL_HEIGHT + NOTE_GAP,
        CLOSING,
        width=CONTENT_WIDTH,
    )
    return scene
