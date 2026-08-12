"""Composite figures built from `Scene` primitives — the deck's vocabulary of forms.

A slide asks for a form, not for rectangles: `matrix`, `layered stack` (in
`decks/golem/lichess_stack.py`), `hub_and_satellites`, `swimlane`,
`state_machine`, `split_compare`, `cluster_map`, `card_rhythm`, `timeline`,
`coverage_bars`, `icon_card_row`. Having ten forms rather than one is the point:
the same shape four slides running reads as one slide shown four times.

Each builder takes a `Scene`, draws into it, and returns the geometry a caller
needs to keep going — the y a header ended at, the `Grid` a matrix laid out on,
the rectangles a row produced. Sizes come from the estimates in text.py, so a
builder can measure its own content and grow.

Font sizes default to `excalidraw.type_scale`. A caller may move a size up the
scale, and should never move one below `CAPTION_SIZE`.
"""

from __future__ import annotations

import math
from dataclasses import dataclass
from typing import Callable, Mapping, NamedTuple, Sequence

from . import icons
from .palette import (
    GAP,
    INK,
    INK_FAINT,
    INK_SOFT,
    NEUTRAL,
    TRANSPARENT,
    WHITE,
    Tone,
)
from .scene import (
    CONTAINER_PADDING,
    CONTENT_WIDTH,
    LABEL_HEADROOM,
    MARGIN,
    Scene,
    bottom_edge,
    centre,
    fit_width,
    right_edge,
)
from .text import HAND, LINE_HEIGHT, MONO, measured_height, measured_width, wrapped
from .type_scale import BODY_SIZE, CAPTION_SIZE, HEADING_SIZE, TITLE_SIZE

TITLE_FONT_SIZE = TITLE_SIZE
SUBTITLE_FONT_SIZE = BODY_SIZE
NOTE_FONT_SIZE = BODY_SIZE
HEADING_FONT_SIZE = HEADING_SIZE
BOX_TITLE_FONT_SIZE = BODY_SIZE
BOX_DETAIL_FONT_SIZE = CAPTION_SIZE
TAG_FONT_SIZE = CAPTION_SIZE
TEXT_CARD_PADDING = 18.0
TEXT_CARD_SPACING = 8.0
TRANSITION_LABEL_CLEARANCE = 14.0

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
            cursor + 8,
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
    font_size: float = CAPTION_SIZE,
    anchor: str = "left",
    height: float | None = None,
    min_width: float = 0.0,
    stroke_style: str = "solid",
    font_family: int = HAND,
) -> dict:
    width = max(fit_width(body, font_size, font_family=font_family), min_width)
    box_height = height if height is not None else font_size * LINE_HEIGHT + 16
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
    height: float = 58,
    font_size: float = BODY_SIZE,
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
    font_size: float = BODY_SIZE,
    dashed: bool = True,
    padding: float = 18,
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
    font_size: float = CAPTION_SIZE,
    swatch: float = 26,
    gap: float = 36,
    vertical: bool = False,
) -> float:
    cursor_x = x
    cursor_y = y
    bottom = y
    for tone, caption in entries:
        scene.rectangle(cursor_x, cursor_y, swatch, swatch, tone)
        scene.text(
            cursor_x + swatch + 12,
            cursor_y + (swatch - font_size * LINE_HEIGHT) / 2.0,
            caption,
            font_size=font_size,
            colour=INK_SOFT,
        )
        bottom = cursor_y + swatch
        if vertical:
            cursor_y += swatch + 14
        else:
            cursor_x += swatch + 12 + measured_width(caption, font_size) + gap
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
    padding: float = 16,
    index_gutter: float = 46,
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
            height=tag_font_size * LINE_HEIGHT + 10,
        )
        inset = tag["width"] + 14
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
    spacing = 8 if detail else 0
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
            width=index_gutter - 10,
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
    gap: float = 14,
    title_font_size: float = BOX_TITLE_FONT_SIZE,
    detail_font_size: float = BOX_DETAIL_FONT_SIZE,
    tag_font_size: float = TAG_FONT_SIZE,
    padding: float = 16,
    align: str = "left",
) -> list[dict]:
    return [
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
        for position, box in enumerate(boxes)
    ]


def box_column(
    scene: Scene,
    x: float,
    y: float,
    width: float,
    boxes: Sequence[LabelledBox],
    *,
    box_height: float,
    gap: float = 16,
    title_font_size: float = BOX_TITLE_FONT_SIZE,
    detail_font_size: float = BOX_DETAIL_FONT_SIZE,
    tag_font_size: float = TAG_FONT_SIZE,
    padding: float = 16,
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
    gap: float = 28,
    title_font_size: float = BOX_TITLE_FONT_SIZE,
    detail_font_size: float = BOX_DETAIL_FONT_SIZE,
    tag_font_size: float = TAG_FONT_SIZE,
    padding: float = 16,
    align: str = "center",
) -> list[dict]:
    return [
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
        for position, box in enumerate(boxes)
    ]


def pipeline(
    scene: Scene,
    x: float,
    y: float,
    boxes: Sequence[LabelledBox],
    *,
    box_width: float,
    box_height: float,
    gap: float = 56,
    arrow_labels: Sequence[str] = (),
    arrow_colour: str = INK_SOFT,
    title_font_size: float = BOX_TITLE_FONT_SIZE,
    detail_font_size: float = BOX_DETAIL_FONT_SIZE,
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
                middle - 14 - CAPTION_SIZE * LINE_HEIGHT,
                caption,
                font_size=CAPTION_SIZE,
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
    font_size: float = CAPTION_SIZE,
    label_offset: float = 10,
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
    padding: float = 22,
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
        cursor = bottom_edge(title) + 16
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
    row_label_width: float = 340,
    header_height: float = 92,
    row_height: float = 72,
    gap: float = 8,
    header_font_size: float = CAPTION_SIZE,
    row_font_size: float = BODY_SIZE,
    cell_font_size: float = CAPTION_SIZE,
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
        laid_out = wrapped(
            caption, (column_width - 16) * LABEL_HEADROOM, header_font_size
        )
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
            caption, (row_label_width - 24) * LABEL_HEADROOM, row_font_size
        )
        height = measured_height(laid_out, row_font_size)
        scene.text(
            x,
            grid.row_y(row) + (row_height - height) / 2.0,
            laid_out,
            font_size=row_font_size,
            colour=INK,
            align="right",
            width=row_label_width - 20,
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


def hub_and_satellites(
    scene: Scene,
    x: float,
    y: float,
    width: float,
    hub: LabelledBox,
    satellites: Sequence[LabelledBox],
    *,
    satellite_height: float,
    hub_width: float = 380,
    gutter: float = 92,
    gap: float = 14,
    hub_title_font_size: float = HEADING_FONT_SIZE,
    satellite_title_font_size: float = BOX_TITLE_FONT_SIZE,
    satellite_detail_font_size: float = BOX_DETAIL_FONT_SIZE,
) -> tuple[dict, list[dict]]:
    span = len(satellites) * satellite_height + (len(satellites) - 1) * gap
    satellite_x = x + hub_width + gutter
    satellite_width = width - hub_width - gutter
    hub_height = min(span, 260.0)
    hub_rect = labelled_box(
        scene,
        x,
        y + (span - hub_height) / 2.0,
        hub_width,
        hub_height,
        hub,
        title_font_size=hub_title_font_size,
        detail_font_size=BOX_DETAIL_FONT_SIZE,
        align="center",
    )
    drawn: list[dict] = []
    for position, satellite in enumerate(satellites):
        top = y + position * (satellite_height + gap)
        drawn.append(
            labelled_box(
                scene,
                satellite_x,
                top,
                satellite_width,
                satellite_height,
                satellite,
                title_font_size=satellite_title_font_size,
                detail_font_size=satellite_detail_font_size,
            )
        )
        connector(
            scene,
            [
                (right_edge(hub_rect) + 6, centre(hub_rect)[1]),
                (satellite_x - 6, top + satellite_height / 2.0),
            ],
            stroke=INK_FAINT,
        )
    return hub_rect, drawn


class Lane(NamedTuple):
    title: str
    stages: Sequence[LabelledBox]
    tone: Tone = NEUTRAL


def swimlane(
    scene: Scene,
    x: float,
    y: float,
    width: float,
    lanes: Sequence[Lane],
    *,
    lane_height: float,
    lane_gap: float = 28,
    stage_gap: float = 56,
    lane_label_width: float = 250,
    lane_title_font_size: float = HEADING_FONT_SIZE,
    stage_title_font_size: float = BOX_TITLE_FONT_SIZE,
    stage_detail_font_size: float = BOX_DETAIL_FONT_SIZE,
) -> list[list[dict]]:
    drawn: list[list[dict]] = []
    for index, lane in enumerate(lanes):
        top = y + index * (lane_height + lane_gap)
        laid_out = wrapped(
            lane.title, (lane_label_width - 24) * LABEL_HEADROOM, lane_title_font_size
        )
        scene.text(
            x,
            top + (lane_height - measured_height(laid_out, lane_title_font_size)) / 2.0,
            laid_out,
            font_size=lane_title_font_size,
            colour=lane.tone.stroke,
            width=lane_label_width - 24,
        )
        stage_area_x = x + lane_label_width
        stage_area_width = width - lane_label_width
        stage_width = (
            stage_area_width - stage_gap * (len(lane.stages) - 1)
        ) / len(lane.stages)
        drawn.append(
            pipeline(
                scene,
                stage_area_x,
                top,
                lane.stages,
                box_width=stage_width,
                box_height=lane_height,
                gap=stage_gap,
                title_font_size=stage_title_font_size,
                detail_font_size=stage_detail_font_size,
            )
        )
    return drawn


class StateNode(NamedTuple):
    key: str
    label: str
    x: float
    y: float
    tone: Tone = NEUTRAL
    width: float = 250.0
    height: float = 90.0
    detail: str = ""


class Transition(NamedTuple):
    source: str
    target: str
    label: str = ""
    bow: float = 0.0
    dashed: bool = False
    stroke: str = INK_SOFT


# NOTE: an arrow between two state boxes has to stop at the box edge, not at its
# centre, or the arrowhead disappears under the fill. Scale the centre-to-centre
# vector until it meets the nearer of the two half-extents.
def _edge_point(rect: dict, towards: tuple[float, float]) -> tuple[float, float]:
    hub_x, hub_y = centre(rect)
    delta_x = towards[0] - hub_x
    delta_y = towards[1] - hub_y
    if delta_x == 0 and delta_y == 0:
        return hub_x, hub_y
    half_width = rect["width"] / 2.0
    half_height = rect["height"] / 2.0
    horizontal = half_width / abs(delta_x) if delta_x else math.inf
    vertical = half_height / abs(delta_y) if delta_y else math.inf
    reach = min(horizontal, vertical)
    return hub_x + delta_x * reach, hub_y + delta_y * reach


def state_machine(
    scene: Scene,
    nodes: Sequence[StateNode],
    transitions: Sequence[Transition],
    *,
    label_font_size: float = CAPTION_SIZE,
    node_font_size: float = BOX_TITLE_FONT_SIZE,
    detail_font_size: float = CAPTION_SIZE,
) -> Mapping[str, dict]:
    drawn: dict[str, dict] = {}
    for node in nodes:
        drawn[node.key] = labelled_box(
            scene,
            node.x,
            node.y,
            node.width,
            node.height,
            LabelledBox(node.label, node.detail, node.tone),
            title_font_size=node_font_size,
            detail_font_size=detail_font_size,
            align="center",
        )
    for transition in transitions:
        source = drawn[transition.source]
        target = drawn[transition.target]
        source_centre = centre(source)
        target_centre = centre(target)
        mid_x = (source_centre[0] + target_centre[0]) / 2.0
        mid_y = (source_centre[1] + target_centre[1]) / 2.0
        span_x = target_centre[0] - source_centre[0]
        span_y = target_centre[1] - source_centre[1]
        length = math.hypot(span_x, span_y) or 1.0
        perpendicular_x = -span_y / length
        perpendicular_y = span_x / length
        bow_x = mid_x + perpendicular_x * transition.bow
        bow_y = mid_y + perpendicular_y * transition.bow
        start = _edge_point(source, (bow_x, bow_y))
        end = _edge_point(target, (bow_x, bow_y))
        points = (
            [start, (bow_x, bow_y), end] if transition.bow else [start, end]
        )
        scene.arrow(
            points,
            stroke=transition.stroke,
            stroke_style="dashed" if transition.dashed else "solid",
        )
        if transition.label:
            # NOTE: the label clears the arrow along the same perpendicular the bow
            # was measured on, never along -y. Both segments radiate from the bow
            # vertex, so a purely vertical offset only clears them while the arrow is
            # locally horizontal: on a vertical transition the arrow was drawn
            # straight through the glyphs. And the sign of `bow` alone does not say
            # which side is outside the arc, because the perpendicular flips with the
            # transition's direction — two arrows between the same pair of boxes, one
            # each way, with the same bow, landed on opposite sides.
            #
            # `reach` is the label box's own extent in the perpendicular direction —
            # its half-width when the transition is vertical, its half-height when
            # horizontal. Offsetting the label's centre by the clearance alone left
            # a vertical transition's arrow crossing the far end of the caption.
            caption_width = measured_width(transition.label, label_font_size)
            caption_height = label_font_size * LINE_HEIGHT
            outward = 1.0 if transition.bow >= 0 else -1.0
            reach = (
                abs(perpendicular_x) * caption_width / 2.0
                + abs(perpendicular_y) * caption_height / 2.0
            )
            clearance = TRANSITION_LABEL_CLEARANCE + reach
            label_x = bow_x + perpendicular_x * outward * clearance
            label_y = bow_y + perpendicular_y * outward * clearance
            scene.text(
                label_x - caption_width / 2.0,
                label_y - caption_height / 2.0,
                transition.label,
                font_size=label_font_size,
                colour=transition.stroke,
                align="center",
                width=caption_width,
            )
    return drawn


def split_compare(
    scene: Scene,
    x: float,
    y: float,
    width: float,
    height: float,
    left: tuple[str, Tone],
    right: tuple[str, Tone],
    *,
    gutter: float = 76,
    heading_font_size: float = HEADING_FONT_SIZE,
) -> tuple[PanelArea, PanelArea]:
    half = (width - gutter) / 2.0
    left_panel = panel(
        scene, x, y, half, height, left[0], tone=left[1],
        heading_font_size=heading_font_size,
    )
    right_panel = panel(
        scene, x + half + gutter, y, half, height, right[0], tone=right[1],
        heading_font_size=heading_font_size,
    )
    divider_x = x + half + gutter / 2.0
    scene.line(
        [(divider_x, y), (divider_x, y + height)],
        stroke=INK_FAINT,
        stroke_style="dashed",
    )
    return left_panel, right_panel


class ClusterNode(NamedTuple):
    title: str
    workloads: int = 0
    tone: Tone = NEUTRAL
    tag: str = ""
    workload_tone: Tone | None = None


def cluster_map(
    scene: Scene,
    x: float,
    y: float,
    width: float,
    nodes: Sequence[ClusterNode],
    *,
    node_height: float,
    gap: float = 32,
    workload_size: float = 44,
    workload_gap: float = 14,
    title_font_size: float = BOX_TITLE_FONT_SIZE,
    tag_font_size: float = TAG_FONT_SIZE,
) -> list[dict]:
    node_width = (width - gap * (len(nodes) - 1)) / len(nodes)
    drawn: list[dict] = []
    for position, node in enumerate(nodes):
        node_x = x + position * (node_width + gap)
        rect = labelled_box(
            scene,
            node_x,
            y,
            node_width,
            node_height,
            LabelledBox(node.title, "", node.tone, node.tag),
            title_font_size=title_font_size,
            tag_font_size=tag_font_size,
            align="center",
        )
        drawn.append(rect)
        if node.workloads:
            mark_width = icons.CONTAINER_ASPECT * workload_size
            span = (
                node.workloads * mark_width + (node.workloads - 1) * workload_gap
            )
            first_x = node_x + (node_width - span) / 2.0
            for index in range(node.workloads):
                icons.container(
                    scene,
                    first_x + index * (mark_width + workload_gap),
                    y + node_height - workload_size - 20,
                    workload_size,
                    tone=node.workload_tone
                    if node.workload_tone is not None
                    else NEUTRAL,
                )
    return drawn


class RhythmCard(NamedTuple):
    weight: float
    box: LabelledBox


def card_rhythm(
    scene: Scene,
    x: float,
    y: float,
    width: float,
    rows: Sequence[Sequence[RhythmCard]],
    *,
    row_height: float,
    row_gap: float = 22,
    card_gap: float = 24,
    title_font_size: float = BOX_TITLE_FONT_SIZE,
    detail_font_size: float = BOX_DETAIL_FONT_SIZE,
) -> list[list[dict]]:
    drawn: list[list[dict]] = []
    for row_index, row in enumerate(rows):
        top = y + row_index * (row_height + row_gap)
        available = width - card_gap * (len(row) - 1)
        total_weight = sum(card.weight for card in row)
        cursor = x
        placed: list[dict] = []
        for card in row:
            card_width = available * card.weight / total_weight
            placed.append(
                labelled_box(
                    scene,
                    cursor,
                    top,
                    card_width,
                    row_height,
                    card.box,
                    title_font_size=title_font_size,
                    detail_font_size=detail_font_size,
                )
            )
            cursor += card_width + card_gap
        drawn.append(placed)
    return drawn


IconDrawer = Callable[..., icons.Mark]


class IconCard(NamedTuple):
    draw: IconDrawer
    aspect: float
    title: str
    detail: str = ""
    tone: Tone = NEUTRAL
    icon_tone: Tone | None = None


def icon_card_row(
    scene: Scene,
    x: float,
    y: float,
    cards: Sequence[IconCard],
    *,
    card_height: float,
    icon_size: float,
    card_width: float | None = None,
    gap: float = 34,
    flow: bool = False,
    padding: float = 20,
    title_font_size: float = BOX_TITLE_FONT_SIZE,
    detail_font_size: float = BOX_DETAIL_FONT_SIZE,
    total_width: float = CONTENT_WIDTH,
) -> list[dict]:
    width = (
        card_width
        if card_width is not None
        else (total_width - gap * (len(cards) - 1)) / len(cards)
    )
    drawn: list[dict] = []
    for position, card in enumerate(cards):
        card_x = x + position * (width + gap)
        rect = scene.rectangle(card_x, y, width, card_height, card.tone)
        drawn.append(rect)
        mark_width = card.aspect * icon_size
        card.draw(
            scene,
            card_x + (width - mark_width) / 2.0,
            y + padding,
            icon_size,
            **({"tone": card.icon_tone} if card.icon_tone is not None else {}),
        )
        text_width = width - 2 * padding
        title = wrapped(card.title, text_width * LABEL_HEADROOM, title_font_size)
        title_height = measured_height(title, title_font_size)
        detail = (
            wrapped(card.detail, text_width * LABEL_HEADROOM, detail_font_size)
            if card.detail
            else ""
        )
        detail_height = measured_height(detail, detail_font_size) if detail else 0.0
        block = title_height + (10 + detail_height if detail else 0.0)
        top = max(
            y + padding + icon_size + 18,
            y + card_height - padding - block,
        )
        scene.text(
            card_x + padding,
            top,
            title,
            font_size=title_font_size,
            colour=card.tone.text,
            align="center",
            width=text_width,
        )
        if detail:
            scene.text(
                card_x + padding,
                top + title_height + 10,
                detail,
                font_size=detail_font_size,
                colour=INK_SOFT,
                align="center",
                width=text_width,
            )
    if flow:
        middle = y + card_height / 2.0
        for position in range(len(drawn) - 1):
            scene.arrow(
                [
                    (right_edge(drawn[position]) + 8, middle),
                    (drawn[position + 1]["x"] - 8, middle),
                ],
                stroke=INK_SOFT,
            )
    return drawn


class Tick(NamedTuple):
    label: str
    caption: str = ""
    tone: Tone = NEUTRAL


def timeline(
    scene: Scene,
    x: float,
    y: float,
    width: float,
    ticks: Sequence[Tick],
    *,
    start_caption: str = "",
    end_caption: str = "",
    label_font_size: float = BODY_SIZE,
    caption_font_size: float = CAPTION_SIZE,
    tick_height: float = 26,
    marker_height: float = 56,
) -> list[float]:
    axis_y = y + marker_height
    scene.arrow(
        [(x, axis_y), (x + width, axis_y)], stroke=INK_SOFT, stroke_width=3
    )
    step = width / len(ticks)
    positions: list[float] = []
    for index, tick in enumerate(ticks):
        centre_x = x + step * (index + 0.5)
        positions.append(centre_x)
        scene.line(
            [(centre_x, axis_y - tick_height / 2.0), (centre_x, axis_y + tick_height / 2.0)],
            stroke=tick.tone.stroke,
            stroke_width=3,
        )
        laid_out = wrapped(tick.label, (step - 18) * LABEL_HEADROOM, label_font_size)
        scene.text(
            centre_x - (step - 18) / 2.0,
            axis_y + tick_height / 2.0 + 14,
            laid_out,
            font_size=label_font_size,
            colour=INK,
            align="center",
            width=step - 18,
        )
        if tick.caption:
            caption = wrapped(
                tick.caption, (step - 18) * LABEL_HEADROOM, caption_font_size
            )
            scene.text(
                centre_x - (step - 18) / 2.0,
                axis_y
                + tick_height / 2.0
                + 14
                + measured_height(laid_out, label_font_size)
                + 8,
                caption,
                font_size=caption_font_size,
                colour=INK_FAINT,
                align="center",
                width=step - 18,
            )
    if start_caption:
        scene.text(
            x,
            axis_y - marker_height,
            start_caption,
            font_size=caption_font_size,
            colour=INK_FAINT,
            width=width / 2.0,
        )
    if end_caption:
        scene.text(
            x + width / 2.0,
            axis_y - marker_height,
            end_caption,
            font_size=caption_font_size,
            colour=INK_FAINT,
            align="right",
            width=width / 2.0,
        )
    return positions


class CoverageRow(NamedTuple):
    label: str
    covered: float
    covered_tone: Tone
    remainder_tone: Tone = GAP
    covered_tag: str = ""
    remainder_tag: str = ""


def coverage_bars(
    scene: Scene,
    x: float,
    y: float,
    width: float,
    rows: Sequence[CoverageRow],
    *,
    bar_height: float,
    gap: float = 14,
    label_width: float = 420,
    label_font_size: float = BODY_SIZE,
    tag_font_size: float = CAPTION_SIZE,
) -> list[tuple[dict, dict | None]]:
    track_x = x + label_width
    track_width = width - label_width
    drawn: list[tuple[dict | None, dict | None]] = []
    for index, row in enumerate(rows):
        top = y + index * (bar_height + gap)
        laid_out = wrapped(
            row.label, (label_width - 24) * LABEL_HEADROOM, label_font_size
        )
        scene.text(
            x,
            top + (bar_height - measured_height(laid_out, label_font_size)) / 2.0,
            laid_out,
            font_size=label_font_size,
            colour=INK,
            align="right",
            width=label_width - 20,
        )
        covered_width = track_width * max(0.0, min(1.0, row.covered))
        covered: dict | None = None
        if covered_width > 1:
            covered = scene.rectangle(
                track_x,
                top,
                covered_width,
                bar_height,
                row.covered_tone,
                label=row.covered_tag if covered_width > 160 else "",
                label_font_size=tag_font_size,
            )
        remainder: dict | None = None
        if covered_width < track_width - 1:
            remainder = scene.rectangle(
                track_x + covered_width,
                top,
                track_width - covered_width,
                bar_height,
                row.remainder_tone,
                label=row.remainder_tag
                if track_width - covered_width > 160
                else "",
                label_font_size=tag_font_size,
            )
        drawn.append((covered, remainder))
    return drawn
