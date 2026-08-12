"""Composite figures built from `Scene` primitives.

A slide should ask for a header, a matrix, a pipeline or a legend — not place
rectangles and text one at a time. Each builder here takes a `Scene`, draws into
it, and returns the geometry a caller needs to keep going: the y a header ended
at, the `Grid` a matrix laid out on, the rectangles a row produced. Sizes come
from the estimates in text.py, so a builder can measure its own content and grow.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import NamedTuple, Sequence

from .palette import INK, INK_FAINT, INK_SOFT, NEUTRAL, TRANSPARENT, WHITE, Tone
from .scene import (
    CONTAINER_PADDING,
    CONTENT_WIDTH,
    LABEL_HEADROOM,
    MARGIN,
    Scene,
    bottom_edge,
    fit_width,
    right_edge,
)
from .text import HAND, LINE_HEIGHT, MONO, measured_height, measured_width, wrapped

TITLE_FONT_SIZE = 34
SUBTITLE_FONT_SIZE = 18
NOTE_FONT_SIZE = 16
HEADING_FONT_SIZE = 22
BOX_TITLE_FONT_SIZE = 20
BOX_DETAIL_FONT_SIZE = 14
TAG_FONT_SIZE = 12
TEXT_CARD_PADDING = 14.0
TEXT_CARD_SPACING = 6.0

TextLine = tuple[str, float, int]


class LabelledBox(NamedTuple):
    title: str
    detail: str = ""
    tone: Tone = NEUTRAL
    tag: str = ""
    index_label: str = ""


class Area(NamedTuple):
    x: float
    y: float
    width: float
    height: float


class PanelArea(NamedTuple):
    rect: dict
    body: Area


@dataclass(frozen=True)
class Grid:
    x: float
    y: float
    row_label_width: float
    header_height: float
    column_width: float
    row_height: float
    columns: int
    rows: int
    gap: float

    def column_x(self, column: int) -> float:
        return self.x + self.row_label_width + column * self.column_width

    def row_y(self, row: int) -> float:
        return self.y + self.header_height + row * self.row_height

    def cell(self, row: int, column: int) -> Area:
        return Area(
            self.column_x(column) + self.gap / 2.0,
            self.row_y(row) + self.gap / 2.0,
            self.column_width - self.gap,
            self.row_height - self.gap,
        )

    def column_span(self, first_column: int, last_column: int) -> Area:
        left = self.column_x(first_column) + self.gap / 2.0
        right = self.column_x(last_column + 1) - self.gap / 2.0
        return Area(left, self.y, right - left, self.header_height)

    @property
    def width(self) -> float:
        return self.row_label_width + self.columns * self.column_width

    @property
    def height(self) -> float:
        return self.header_height + self.rows * self.row_height

    @property
    def right(self) -> float:
        return self.x + self.width

    @property
    def bottom(self) -> float:
        return self.y + self.height


def slide_header(
    scene: Scene,
    title: str,
    subtitle: str = "",
    *,
    x: float = MARGIN,
    y: float = MARGIN,
    width: float = CONTENT_WIDTH,
    title_font_size: float = TITLE_FONT_SIZE,
    subtitle_font_size: float = SUBTITLE_FONT_SIZE,
) -> float:
    heading = scene.text(
        x,
        y,
        title,
        font_size=title_font_size,
        colour=INK,
        width=width,
        wrap_width=width,
    )
    cursor = bottom_edge(heading)
    if subtitle:
        strapline = scene.text(
            x,
            cursor + 10,
            subtitle,
            font_size=subtitle_font_size,
            colour=INK_SOFT,
            width=width,
            wrap_width=width,
        )
        cursor = bottom_edge(strapline)
    return cursor


def note(
    scene: Scene,
    x: float,
    y: float,
    body: str,
    *,
    width: float = CONTENT_WIDTH,
    font_size: float = NOTE_FONT_SIZE,
    colour: str = INK_SOFT,
    align: str = "left",
    font_family: int = HAND,
) -> dict:
    return scene.text(
        x,
        y,
        body,
        font_size=font_size,
        colour=colour,
        align=align,
        width=width,
        wrap_width=width,
        font_family=font_family,
    )


def badge(
    scene: Scene,
    x: float,
    y: float,
    body: str,
    *,
    tone: Tone,
    font_size: float = 14,
    anchor: str = "left",
    height: float | None = None,
    min_width: float = 0.0,
    stroke_style: str = "solid",
    font_family: int = HAND,
) -> dict:
    width = max(fit_width(body, font_size, font_family=font_family), min_width)
    box_height = height if height is not None else font_size * LINE_HEIGHT + 12
    if anchor == "right":
        x -= width
    elif anchor == "center":
        x -= width / 2.0
    return scene.rectangle(
        x,
        y,
        width,
        box_height,
        tone,
        label=body,
        label_font_size=font_size,
        label_font_family=font_family,
        stroke_style=stroke_style,
    )


def span_bar(
    scene: Scene,
    x: float,
    y: float,
    width: float,
    body: str,
    *,
    tone: Tone,
    height: float = 40,
    font_size: float = 15,
    stroke_style: str = "solid",
) -> dict:
    return scene.rectangle(
        x,
        y,
        width,
        height,
        tone,
        label=body,
        label_font_size=font_size,
        stroke_style=stroke_style,
    )


def callout(
    scene: Scene,
    x: float,
    y: float,
    width: float,
    body: str,
    *,
    tone: Tone = NEUTRAL,
    font_size: float = 15,
    dashed: bool = True,
    padding: float = 14,
) -> dict:
    laid_out = wrapped(body, (width - 2 * CONTAINER_PADDING) * LABEL_HEADROOM, font_size)
    height = measured_height(laid_out, font_size) + 2 * padding
    return scene.rectangle(
        x,
        y,
        width,
        height,
        tone,
        stroke_style="dashed" if dashed else "solid",
        label=laid_out,
        label_font_size=font_size,
        label_wrap=False,
    )


def text_card(
    scene: Scene,
    x: float,
    y: float,
    width: float,
    lines: Sequence[TextLine],
    tone: Tone,
    *,
    height: float | None = None,
    padding: float = TEXT_CARD_PADDING,
    spacing: float = TEXT_CARD_SPACING,
    align: str = "left",
) -> dict:
    text_width = width - 2 * padding
    laid_out = [
        (
            body if family == MONO else wrapped(body, text_width * LABEL_HEADROOM, size),
            size,
            family,
        )
        for body, size, family in lines
    ]
    line_heights = [measured_height(body, size) for body, size, _ in laid_out]
    block = sum(line_heights) + spacing * (len(line_heights) - 1)
    box_height = block + 2 * padding if height is None else height
    rect = scene.rectangle(x, y, width, box_height, tone)
    cursor = y + max(padding, (box_height - block) / 2.0)
    for (body, size, family), line_height in zip(laid_out, line_heights):
        scene.text(
            x + padding,
            cursor,
            body,
            font_size=size,
            colour=INK if family == MONO else INK_SOFT,
            font_family=family,
            align=align,
            width=text_width,
        )
        cursor += line_height + spacing
    return rect


def legend(
    scene: Scene,
    x: float,
    y: float,
    entries: Sequence[tuple[Tone, str]],
    *,
    font_size: float = 15,
    swatch: float = 22,
    gap: float = 30,
    vertical: bool = False,
) -> float:
    cursor_x = x
    cursor_y = y
    bottom = y
    for tone, caption in entries:
        scene.rectangle(cursor_x, cursor_y, swatch, swatch, tone)
        scene.text(
            cursor_x + swatch + 10,
            cursor_y + (swatch - font_size * LINE_HEIGHT) / 2.0,
            caption,
            font_size=font_size,
            colour=INK_SOFT,
        )
        bottom = cursor_y + swatch
        if vertical:
            cursor_y += swatch + 12
        else:
            cursor_x += swatch + 10 + measured_width(caption, font_size) + gap
    return bottom


def labelled_box(
    scene: Scene,
    x: float,
    y: float,
    width: float,
    height: float,
    box: LabelledBox,
    *,
    title_font_size: float = BOX_TITLE_FONT_SIZE,
    detail_font_size: float = BOX_DETAIL_FONT_SIZE,
    tag_font_size: float = TAG_FONT_SIZE,
    padding: float = 14,
    index_gutter: float = 38,
    align: str = "left",
    radius: bool = True,
    stroke_style: str = "solid",
    detail_colour: str | None = None,
) -> dict:
    rect = scene.rectangle(
        x, y, width, height, box.tone, radius=radius, stroke_style=stroke_style
    )
    text_x = x + padding
    text_width = width - 2 * padding
    if box.index_label:
        text_x += index_gutter
        text_width -= index_gutter
    if box.tag:
        tag = badge(
            scene,
            x + width - padding,
            y + padding,
            box.tag,
            tone=Tone(box.tone.stroke, WHITE, box.tone.stroke),
            font_size=tag_font_size,
            anchor="right",
            height=tag_font_size * LINE_HEIGHT + 8,
        )
        inset = tag["width"] + 12
        text_width -= inset
        if align == "center":
            text_x += inset
            text_width -= inset
    title = wrapped(box.title, text_width * LABEL_HEADROOM, title_font_size)
    title_height = measured_height(title, title_font_size)
    detail = (
        wrapped(box.detail, text_width * LABEL_HEADROOM, detail_font_size)
        if box.detail
        else ""
    )
    detail_height = measured_height(detail, detail_font_size) if detail else 0.0
    spacing = 6 if detail else 0
    block_height = title_height + spacing + detail_height
    top = y + max(padding, (height - block_height) / 2.0)
    scene.text(
        text_x,
        top,
        title,
        font_size=title_font_size,
        colour=box.tone.text,
        align=align,
        width=text_width,
    )
    if detail:
        scene.text(
            text_x,
            top + title_height + spacing,
            detail,
            font_size=detail_font_size,
            colour=detail_colour if detail_colour is not None else INK_SOFT,
            align=align,
            width=text_width,
        )
    if box.index_label:
        scene.text(
            x + padding,
            y + (height - title_height) / 2.0,
            box.index_label,
            font_size=title_font_size,
            colour=box.tone.stroke,
            align="left",
            width=index_gutter - 8,
        )
    return rect


def box_stack(
    scene: Scene,
    x: float,
    y: float,
    width: float,
    boxes: Sequence[LabelledBox],
    *,
    box_height: float,
    gap: float = 12,
    title_font_size: float = BOX_TITLE_FONT_SIZE,
    detail_font_size: float = BOX_DETAIL_FONT_SIZE,
    tag_font_size: float = TAG_FONT_SIZE,
    padding: float = 14,
    align: str = "left",
) -> list[dict]:
    drawn: list[dict] = []
    for position, box in enumerate(boxes):
        drawn.append(
            labelled_box(
                scene,
                x,
                y + position * (box_height + gap),
                width,
                box_height,
                box,
                title_font_size=title_font_size,
                detail_font_size=detail_font_size,
                tag_font_size=tag_font_size,
                padding=padding,
                align=align,
            )
        )
    return drawn


def box_column(
    scene: Scene,
    x: float,
    y: float,
    width: float,
    boxes: Sequence[LabelledBox],
    *,
    box_height: float,
    gap: float = 14,
    title_font_size: float = 18,
    detail_font_size: float = 14,
    tag_font_size: float = TAG_FONT_SIZE,
    padding: float = 14,
) -> list[dict]:
    return box_stack(
        scene,
        x,
        y,
        width,
        boxes,
        box_height=box_height,
        gap=gap,
        title_font_size=title_font_size,
        detail_font_size=detail_font_size,
        tag_font_size=tag_font_size,
        padding=padding,
    )


def box_row(
    scene: Scene,
    x: float,
    y: float,
    boxes: Sequence[LabelledBox],
    *,
    box_width: float,
    box_height: float,
    gap: float = 24,
    title_font_size: float = 18,
    detail_font_size: float = 13,
    tag_font_size: float = TAG_FONT_SIZE,
    padding: float = 14,
    align: str = "center",
) -> list[dict]:
    drawn: list[dict] = []
    for position, box in enumerate(boxes):
        drawn.append(
            labelled_box(
                scene,
                x + position * (box_width + gap),
                y,
                box_width,
                box_height,
                box,
                title_font_size=title_font_size,
                detail_font_size=detail_font_size,
                tag_font_size=tag_font_size,
                padding=padding,
                align=align,
            )
        )
    return drawn


def pipeline(
    scene: Scene,
    x: float,
    y: float,
    boxes: Sequence[LabelledBox],
    *,
    box_width: float,
    box_height: float,
    gap: float = 52,
    arrow_labels: Sequence[str] = (),
    arrow_colour: str = INK_SOFT,
    title_font_size: float = 17,
    detail_font_size: float = 12,
    tag_font_size: float = TAG_FONT_SIZE,
    align: str = "center",
) -> list[dict]:
    drawn = box_row(
        scene,
        x,
        y,
        boxes,
        box_width=box_width,
        box_height=box_height,
        gap=gap,
        title_font_size=title_font_size,
        detail_font_size=detail_font_size,
        tag_font_size=tag_font_size,
        align=align,
    )
    middle = y + box_height / 2.0
    for position in range(len(drawn) - 1):
        start = right_edge(drawn[position]) + 8
        end = drawn[position + 1]["x"] - 8
        scene.arrow([(start, middle), (end, middle)], stroke=arrow_colour)
        if position < len(arrow_labels) and arrow_labels[position]:
            caption = arrow_labels[position]
            scene.text(
                start,
                middle - 12 - 12 * LINE_HEIGHT,
                caption,
                font_size=12,
                colour=INK_FAINT,
                align="center",
                width=end - start,
            )
    return drawn


def connector(
    scene: Scene,
    points: Sequence[tuple[float, float]],
    *,
    stroke: str = INK_SOFT,
    stroke_width: float = 2,
    dashed: bool = False,
    arrowhead: bool = True,
    start_arrowhead: str | None = None,
    label: str = "",
    font_size: float = 13,
    label_offset: float = 8,
) -> dict:
    element = scene.arrow(
        points,
        stroke=stroke,
        stroke_width=stroke_width,
        stroke_style="dashed" if dashed else "solid",
        start_arrowhead=start_arrowhead,
        end_arrowhead="arrow" if arrowhead else None,
    )
    if label:
        segments = list(zip(points, points[1:]))
        first, second = max(
            segments,
            key=lambda pair: (pair[1][0] - pair[0][0]) ** 2
            + (pair[1][1] - pair[0][1]) ** 2,
        )
        mid_x = (first[0] + second[0]) / 2.0
        mid_y = (first[1] + second[1]) / 2.0
        caption_width = measured_width(label, font_size)
        scene.text(
            mid_x - caption_width / 2.0,
            mid_y - label_offset - font_size * LINE_HEIGHT,
            label,
            font_size=font_size,
            colour=stroke,
            align="center",
            width=caption_width,
        )
    return element


def panel(
    scene: Scene,
    x: float,
    y: float,
    width: float,
    height: float,
    heading: str,
    *,
    tone: Tone = NEUTRAL,
    heading_font_size: float = HEADING_FONT_SIZE,
    padding: float = 20,
    fill: str = TRANSPARENT,
    stroke_style: str = "solid",
) -> PanelArea:
    rect = scene.rectangle(
        x,
        y,
        width,
        height,
        Tone(tone.stroke, fill, tone.text),
        stroke_style=stroke_style,
    )
    cursor = y + padding
    if heading:
        title = scene.text(
            x + padding,
            cursor,
            heading,
            font_size=heading_font_size,
            colour=tone.stroke,
            width=width - 2 * padding,
            wrap_width=width - 2 * padding,
        )
        cursor = bottom_edge(title) + 14
    return PanelArea(
        rect,
        Area(x + padding, cursor, width - 2 * padding, y + height - padding - cursor),
    )


def matrix(
    scene: Scene,
    x: float,
    y: float,
    *,
    column_labels: Sequence[str],
    row_labels: Sequence[str],
    tones: Sequence[Sequence[Tone]],
    cell_labels: Sequence[Sequence[str]] | None = None,
    total_width: float = CONTENT_WIDTH,
    row_label_width: float = 260,
    header_height: float = 66,
    row_height: float = 60,
    gap: float = 8,
    header_font_size: float = 15,
    row_font_size: float = 15,
    cell_font_size: float = 13,
) -> Grid:
    column_width = (total_width - row_label_width) / len(column_labels)
    grid = Grid(
        x=x,
        y=y,
        row_label_width=row_label_width,
        header_height=header_height,
        column_width=column_width,
        row_height=row_height,
        columns=len(column_labels),
        rows=len(row_labels),
        gap=gap,
    )
    for column, caption in enumerate(column_labels):
        laid_out = wrapped(caption, (column_width - 16) * LABEL_HEADROOM, header_font_size)
        height = measured_height(laid_out, header_font_size)
        scene.text(
            grid.column_x(column) + gap / 2.0,
            y + header_height - height - 10,
            laid_out,
            font_size=header_font_size,
            colour=INK,
            align="center",
            width=column_width - gap,
        )
    for row, caption in enumerate(row_labels):
        laid_out = wrapped(
            caption, (row_label_width - 20) * LABEL_HEADROOM, row_font_size
        )
        height = measured_height(laid_out, row_font_size)
        scene.text(
            x,
            grid.row_y(row) + (row_height - height) / 2.0,
            laid_out,
            font_size=row_font_size,
            colour=INK,
            align="right",
            width=row_label_width - 16,
        )
    for row in range(grid.rows):
        for column in range(grid.columns):
            area = grid.cell(row, column)
            caption = (
                cell_labels[row][column]
                if cell_labels is not None and cell_labels[row][column]
                else ""
            )
            scene.rectangle(
                area.x,
                area.y,
                area.width,
                area.height,
                tones[row][column],
                label=caption,
                label_font_size=cell_font_size,
            )
    return grid
