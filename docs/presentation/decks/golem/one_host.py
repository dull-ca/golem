from __future__ import annotations

from typing import NamedTuple

from excalidraw.layout import Tick, connector, timeline
from excalidraw.palette import (
    GOLEM,
    INK_GHOST,
    INK_SOFT,
    ORANGE,
    ORANGE_FILL,
    WHITE,
    Tone,
)
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.text import MONO
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE, HEADING_SIZE

from ..machines import Machine, cell_area, draw_machine, golem_mark, scroll_mark
from . import exemplar, glyph_kinds
from .glyph_ops import REMOVE, Op

CONTENT_TOP = 206.0
CONTENT_BOTTOM = 936.0
HEADER_CLEARANCE = 12.0

MACHINE_TONE = Tone(INK_SOFT, WHITE)
GHOST_TONE = Tone(INK_GHOST, WHITE)
RECORD_TONE = Tone(ORANGE, WHITE)
RECORD_FILLED = Tone(ORANGE, ORANGE_FILL)

SCROLL_WIDTH = 360.0
SCROLL_HEIGHT = 76.0
SCROLL_X = MARGIN + (CONTENT_WIDTH - SCROLL_WIDTH) / 2.0
SCROLL_CENTRE_X = SCROLL_X + SCROLL_WIDTH / 2.0

BOX_WIDTH = 1200.0
BOX_X = MARGIN + (CONTENT_WIDTH - BOX_WIDTH) / 2.0
NAME_SIZE = HEADING_SIZE

PORTRAIT_Y = 311.0
PORTRAIT_HEIGHT = 520.0

LANDING_Y = 585.0
LANDING_HEIGHT = 260.0

BADGE_SIZE = 36.0
BADGE_INSET = 26.0

STRIP_HEIGHT = 60.0
STRIP_INSET = 40.0
STRIP_CLEARANCE = 14.0
STRIP_LABEL = "golemd"
STRIP_CAPTION_GAP = 12.0

GLYPH_ICON = 76.0
GLYPH_MARK = 40.0

RECORD_HEADING_Y = 206.0
TIMELINE_Y = 244.0
MARKER_HEIGHT = 92.0
TICK_HEIGHT = 26.0
TICK_ICON = 56.0
TICK_MARK = 32.0
TICK_MARK_GAP = 12.0
TICK_ICON_GAP = 10.0

AXIS_Y = TIMELINE_Y + MARKER_HEIGHT
CARD_WIDTH = 300.0
CARD_HEIGHT = 46.0
CARD_Y = (
    AXIS_Y
    + TICK_HEIGHT / 2.0
    + 14
    + BODY_SIZE * 1.25
    + 8
    + CAPTION_SIZE * 1.25
    + 8
)
REPLAY_Y = CARD_Y + CARD_HEIGHT + 8
REPLAY_LABEL = "revision 2   remove"

NOTE_Y = 862.0


class Record(NamedTuple):
    ticks: tuple[float, ...]
    cards: tuple[dict, ...]
    replay: dict | None


def check_header(header_bottom: float) -> None:
    if header_bottom > CONTENT_TOP - HEADER_CLEARANCE:
        raise ValueError(
            f"the header runs to y={header_bottom:.0f} and the figure starts at "
            f"y={CONTENT_TOP:.0f}: shorten the title or the subtitle"
        )


def interior(y: float, height: float) -> tuple[float, float, float, float]:
    return cell_area(BOX_X, y, BOX_WIDTH, height, NAME_SIZE)


def strip_rect(y: float, height: float, captioned: bool) -> tuple[float, float, float]:
    left, top, width, area_height = interior(y, height)
    bottom = top + area_height - STRIP_CLEARANCE
    if captioned:
        bottom -= CAPTION_SIZE * 1.25 + STRIP_CAPTION_GAP
    return (left + STRIP_INSET, bottom - STRIP_HEIGHT, width - 2 * STRIP_INSET)


def glyph_slot(y: float, height: float, index: int, count: int) -> tuple[float, float]:
    left, top, width, _ = interior(y, height)
    _, strip_y, _ = strip_rect(y, height, False)
    pitch = width / count
    return (
        left + pitch * (index + 0.5) - GLYPH_ICON / 2.0,
        top + (strip_y - top - GLYPH_ICON) / 2.0,
    )


def draw_scroll(scene: Scene, y: float) -> dict:
    return scroll_mark(
        scene,
        SCROLL_X,
        y,
        SCROLL_WIDTH,
        SCROLL_HEIGHT,
        exemplar.HOST,
        GOLEM,
        font_size=BODY_SIZE,
    )


def draw_box(
    scene: Scene,
    y: float,
    height: float,
    *,
    golemd: bool,
    caption: str = "",
) -> dict:
    body = draw_machine(
        scene,
        BOX_X,
        y,
        Machine(exemplar.HOST, keeper=MACHINE_TONE),
        width=BOX_WIDTH,
        height=height,
        name_font_size=NAME_SIZE,
    )
    if not golemd:
        return body
    golem_mark(
        scene,
        BOX_X + BOX_WIDTH - BADGE_INSET - BADGE_SIZE,
        y + BADGE_INSET,
        BADGE_SIZE,
        GOLEM,
    )
    left, top, width = strip_rect(y, height, bool(caption))
    scene.rectangle(
        left,
        top,
        width,
        STRIP_HEIGHT,
        GOLEM,
        label=STRIP_LABEL,
        label_font_size=BODY_SIZE,
        label_font_family=MONO,
    )
    if caption:
        scene.text(
            left,
            top + STRIP_HEIGHT + STRIP_CAPTION_GAP,
            caption,
            font_size=CAPTION_SIZE,
            colour=INK_SOFT,
            align="center",
            width=width,
        )
    return body


def draw_glyphs_on_box(
    scene: Scene, y: float, height: float, withdrawn: exemplar.Entry | None
) -> None:
    for index, entry in enumerate(exemplar.GLYPHS):
        left, top = glyph_slot(y, height, index, len(exemplar.GLYPHS))
        gone = entry is withdrawn
        entry.kind.draw(
            scene,
            left,
            top,
            GLYPH_ICON,
            tone=GHOST_TONE if gone else glyph_kinds.GLYPH_TONE,
        )
        if gone:
            REMOVE.mark(
                scene,
                left + (GLYPH_ICON - GLYPH_MARK) / 2.0,
                top + (GLYPH_ICON - GLYPH_MARK) / 2.0,
                GLYPH_MARK,
                REMOVE.tone,
            )


def _tick_head(scene: Scene, centre_x: float, entry: exemplar.Entry, op: Op) -> None:
    span = TICK_MARK + TICK_MARK_GAP + TICK_ICON
    left = centre_x - span / 2.0
    top = AXIS_Y - TICK_HEIGHT / 2.0 - TICK_ICON_GAP - TICK_ICON
    op.mark(scene, left, top + (TICK_ICON - TICK_MARK) / 2.0, TICK_MARK, op.tone)
    entry.kind.draw(
        scene,
        left + TICK_MARK + TICK_MARK_GAP,
        top,
        TICK_ICON,
        tone=glyph_kinds.GLYPH_TONE,
    )


def draw_record(
    scene: Scene,
    op: Op,
    *,
    revision: str,
    replayed: exemplar.Entry | None = None,
) -> Record:
    scene.text(
        MARGIN,
        RECORD_HEADING_Y,
        revision,
        font_size=BODY_SIZE,
        colour=ORANGE,
        width=CONTENT_WIDTH,
    )
    positions = timeline(
        scene,
        MARGIN,
        TIMELINE_Y,
        CONTENT_WIDTH,
        tuple(
            Tick(entry.spelling, entry.target, RECORD_TONE)
            for entry in exemplar.GLYPHS
        ),
        label_font_size=BODY_SIZE,
        caption_font_size=CAPTION_SIZE,
        label_font_family=MONO,
        caption_font_family=MONO,
        tick_height=TICK_HEIGHT,
        marker_height=MARKER_HEIGHT,
    )
    cards: list[dict] = []
    for centre_x, entry in zip(positions, exemplar.GLYPHS):
        _tick_head(scene, centre_x, entry, op)
        cards.append(
            scene.rectangle(
                centre_x - CARD_WIDTH / 2.0,
                CARD_Y,
                CARD_WIDTH,
                CARD_HEIGHT,
                RECORD_FILLED if entry is replayed else RECORD_TONE,
                stroke_width=3 if entry is replayed else 2,
                label=entry.inverse,
                label_font_size=CAPTION_SIZE,
                label_font_family=MONO,
                label_colour=ORANGE,
            )
        )
    replay: dict | None = None
    if replayed is not None:
        centre_x = positions[exemplar.GLYPHS.index(replayed)]
        replay = scene.rectangle(
            centre_x - CARD_WIDTH / 2.0,
            REPLAY_Y,
            CARD_WIDTH,
            CARD_HEIGHT,
            RECORD_FILLED,
            label=REPLAY_LABEL,
            label_font_size=CAPTION_SIZE,
            label_font_family=MONO,
            label_colour=ORANGE,
        )
        connector(
            scene,
            [(centre_x, REPLAY_Y + CARD_HEIGHT + 6), (centre_x, LANDING_Y - 8)],
            stroke=REMOVE.tone.stroke,
            stroke_width=3,
        )
    return Record(tuple(positions), tuple(cards), replay)


def record_frame(
    scene: Scene,
    op: Op,
    header_bottom: float,
    *,
    revision: str,
    replayed: exemplar.Entry | None = None,
) -> Record:
    check_header(header_bottom)
    record = draw_record(scene, op, revision=revision, replayed=replayed)
    draw_box(scene, LANDING_Y, LANDING_HEIGHT, golemd=True)
    draw_glyphs_on_box(scene, LANDING_Y, LANDING_HEIGHT, replayed)
    return record
