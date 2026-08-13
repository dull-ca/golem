from __future__ import annotations

from dataclasses import dataclass
from typing import Mapping, NamedTuple

from excalidraw.layout import LabelledBox, badge, labelled_box
from excalidraw.palette import INK_SOFT, NEUTRAL, TRANSPARENT, WHITE, Tone
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.text import measured_height, wrapped
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE

from ..vocabulary import ORCHESTRATION_PARTS


class BandSpec(NamedTuple):
    number: int
    title: str
    detail: str


BANDS: tuple[BandSpec, ...] = (
    BandSpec(1, "Facility and power", "the building, the power, the cooling"),
    BandSpec(2, "Bare metal", "the machine, its disks and its network card"),
    BandSpec(3, "Network", "links, addresses, the private network"),
    BandSpec(4, "Operating system", "kernel, users, firewall, ssh"),
    BandSpec(5, "Application hosting", "container runtime, systemd, storage"),
    BandSpec(6, "Tools, dependencies, runtimes", "JVM, node, native libs, base images"),
    BandSpec(7, "The applications", "the processes that serve users"),
)

BOUGHT_BANDS = (1, 2, 3)
CONFIGURED_BANDS = (4, 5, 6, 7)
COLUMN_BANDS = (5, 6, 7)

ORIGIN_X = MARGIN
ORIGIN_Y = 176.0
STACK_WIDTH = 900.0
HEIGHT = 672.0
BAND_GAP = 12.0
BAND_HEIGHT = (HEIGHT - (len(BANDS) - 1) * BAND_GAP) / len(BANDS)
COLUMN_WIDTH = 300.0
COLUMN_GAP = 20.0
BAND_WIDTH = STACK_WIDTH - COLUMN_WIDTH - COLUMN_GAP
COLUMN_X = ORIGIN_X + STACK_WIDTH - COLUMN_WIDTH
BOTTOM = ORIGIN_Y + HEIGHT

GUTTER_X = ORIGIN_X + STACK_WIDTH + 40.0
GUTTER_WIDTH = MARGIN + CONTENT_WIDTH - GUTTER_X
LANES = 3
LANE_GAP = 10.0
LANE_WIDTH = (GUTTER_WIDTH - (LANES - 1) * LANE_GAP) / LANES

BAND_PADDING = 10.0
BAND_INDEX_GUTTER = 46.0
COLUMN_PADDING = 14.0
PART_GAP = 6.0


@dataclass(frozen=True)
class Figure:
    bands: Mapping[int, dict]
    column: dict
    parts: Mapping[int, dict]

    @property
    def bottom(self) -> float:
        return BOTTOM


def band_top(number: int) -> float:
    return ORIGIN_Y + (len(BANDS) - number) * (BAND_HEIGHT + BAND_GAP)


def band_bottom(number: int) -> float:
    return band_top(number) + BAND_HEIGHT


def band_span(lowest: int, highest: int) -> tuple[float, float]:
    top = band_top(highest)
    return top, band_bottom(lowest) - top


def lane_x(lane: int) -> float:
    return GUTTER_X + lane * (LANE_WIDTH + LANE_GAP)


def lane_span(first_lane: int, last_lane: int) -> tuple[float, float]:
    left = lane_x(first_lane)
    return left, lane_x(last_lane) + LANE_WIDTH - left


def _band_is_wide(number: int) -> bool:
    return number not in COLUMN_BANDS


def draw(
    scene: Scene,
    *,
    band_tones: Mapping[int, Tone] | None = None,
    band_tags: Mapping[int, str] | None = None,
    column_tone: Tone = NEUTRAL,
    column_tag: str = "",
    part_tones: Mapping[int, Tone] | None = None,
    part_stroke_styles: Mapping[int, str] | None = None,
    default_band_tone: Tone = NEUTRAL,
    default_part_tone: Tone = NEUTRAL,
) -> Figure:
    drawn_bands: dict[int, dict] = {}
    for spec in reversed(BANDS):
        drawn_bands[spec.number] = labelled_box(
            scene,
            ORIGIN_X,
            band_top(spec.number),
            STACK_WIDTH if _band_is_wide(spec.number) else BAND_WIDTH,
            BAND_HEIGHT,
            LabelledBox(
                title=spec.title,
                detail=spec.detail,
                tone=(band_tones or {}).get(spec.number, default_band_tone),
                tag=(band_tags or {}).get(spec.number, ""),
                index_label=str(spec.number),
            ),
            title_font_size=BODY_SIZE,
            detail_font_size=CAPTION_SIZE,
            tag_font_size=CAPTION_SIZE,
            padding=BAND_PADDING,
            index_gutter=BAND_INDEX_GUTTER,
        )

    column_top, column_height = band_span(min(COLUMN_BANDS), max(COLUMN_BANDS))
    column = scene.rectangle(
        COLUMN_X, column_top, COLUMN_WIDTH, column_height, column_tone
    )

    header = wrapped("Orchestration", COLUMN_WIDTH - 2 * COLUMN_PADDING, BODY_SIZE)
    scene.text(
        COLUMN_X + COLUMN_PADDING,
        column_top + COLUMN_PADDING,
        header,
        font_size=BODY_SIZE,
        colour=column_tone.text,
        align="center",
        width=COLUMN_WIDTH - 2 * COLUMN_PADDING,
    )
    cursor = column_top + COLUMN_PADDING + measured_height(header, BODY_SIZE) + 6

    if column_tag:
        chip = badge(
            scene,
            COLUMN_X + COLUMN_WIDTH / 2.0,
            cursor,
            column_tag,
            tone=Tone(column_tone.stroke, WHITE, column_tone.stroke),
            font_size=CAPTION_SIZE,
            anchor="center",
        )
        cursor += chip["height"] + 6

    part_height = (
        column_top
        + column_height
        - COLUMN_PADDING
        - cursor
        - PART_GAP * (len(ORCHESTRATION_PARTS) - 1)
    ) / len(ORCHESTRATION_PARTS)

    drawn_parts: dict[int, dict] = {}
    for position, part in enumerate(ORCHESTRATION_PARTS):
        drawn_parts[part.number] = labelled_box(
            scene,
            COLUMN_X + COLUMN_PADDING,
            cursor + position * (part_height + PART_GAP),
            COLUMN_WIDTH - 2 * COLUMN_PADDING,
            part_height,
            LabelledBox(
                title=part.title,
                tone=(part_tones or {}).get(part.number, default_part_tone),
            ),
            title_font_size=CAPTION_SIZE,
            padding=8,
            align="center",
            stroke_style=(part_stroke_styles or {}).get(part.number, "solid"),
        )

    return Figure(bands=drawn_bands, column=column, parts=drawn_parts)


def gutter_bar(
    scene: Scene,
    lanes: tuple[int, int],
    bands: tuple[int, int],
    title: str,
    tone: Tone,
    *,
    detail: str = "",
    stroke_style: str = "solid",
) -> dict:
    left, width = lane_span(*lanes)
    top, height = band_span(*bands)
    return labelled_box(
        scene,
        left,
        top,
        width,
        height,
        LabelledBox(title=title, detail=detail, tone=tone),
        title_font_size=BODY_SIZE,
        detail_font_size=CAPTION_SIZE,
        padding=10,
        align="center",
        stroke_style=stroke_style,
    )


def enclose(
    scene: Scene,
    lanes: tuple[int, int],
    bands: tuple[int, int],
    caption: str,
    *,
    headroom: float = 44.0,
) -> None:
    left, width = lane_span(*lanes)
    top, height = band_span(*bands)
    outer_left = left - 6.0
    outer_right = min(left + width + 6.0, MARGIN + CONTENT_WIDTH)
    scene.rectangle(
        outer_left,
        top - headroom,
        outer_right - outer_left,
        height + headroom + 4.0,
        Tone(INK_SOFT, TRANSPARENT),
        stroke_style="dashed",
    )
    scene.text(
        outer_left + 8.0,
        top - headroom + 8.0,
        caption,
        font_size=CAPTION_SIZE,
        colour=INK_SOFT,
        width=outer_right - outer_left - 16.0,
    )
