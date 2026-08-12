"""The six-layer lichess figure, drawn once at one fixed size by two slides.

Layer 6 is a column beside bands 2 to 5, not a band above them, because
orchestration acts across those layers rather than sitting on top of them.
Layer 1 is the only band drawn full width — it runs under the column too.

`draw()` takes tones and tags but no geometry. Slide 04 introduces the figure
and slide 06 recolours it, and the two have to be pixel-identical so that
flipping between them changes colour and nothing else; an earlier draft passed
a different height on each of four slides and the figure jumped. The constants
below are that geometry, and they are not parameters.

Band details are trimmed to what fits at BODY_SIZE on one line. The fuller
enumerations they came from are in SPEC.md.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Mapping, NamedTuple

from excalidraw.layout import LabelledBox, badge, labelled_box
from excalidraw.palette import (
    BLUE,
    BLUE_FILL,
    GREEN,
    GREEN_FILL,
    NEUTRAL,
    ORANGE,
    ORANGE_FILL,
    SLATE,
    SLATE_FILL,
    TEAL,
    TEAL_FILL,
    VIOLET,
    VIOLET_FILL,
    WHITE,
    Tone,
)
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.text import measured_height, wrapped
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE, HEADING_SIZE

from ..vocabulary import ORCHESTRATION_PARTS


class LayerSpec(NamedTuple):
    number: int
    title: str
    detail: str


BAND_LAYERS: tuple[LayerSpec, ...] = (
    LayerSpec(1, "Core OS, network, security", "Debian, kernel, sshd, nftables, TLS"),
    LayerSpec(
        2, "Application hosting", "podman, systemd, storage, registry access"
    ),
    LayerSpec(
        3, "Connective infrastructure", "DNS, SRV records, proxies, load balancers"
    ),
    LayerSpec(
        4, "Tools, dependencies, runtimes", "JVM, node, native libs, base images"
    ),
    LayerSpec(
        5, "The applications", "lila, lila-ws, lila-search, mongodb, redis"
    ),
)

ORCHESTRATION_LAYER = LayerSpec(
    6, "Lifecycle / schedule / scaling", "acts across layers 2 to 5"
)

DESCRIPTIVE_LAYER_TONES: Mapping[int, Tone] = {
    1: Tone(SLATE, SLATE_FILL),
    2: Tone(TEAL, TEAL_FILL),
    3: Tone(BLUE, BLUE_FILL),
    4: Tone(VIOLET, VIOLET_FILL),
    5: Tone(GREEN, GREEN_FILL),
    6: Tone(ORANGE, ORANGE_FILL),
}

DESCRIPTIVE_PART_TONE = Tone(ORANGE, WHITE)

ORIGIN_X = MARGIN
ORIGIN_Y = 190.0
WIDTH = CONTENT_WIDTH
HEIGHT = 604.0
COLUMN_WIDTH = 470.0
COLUMN_GAP = 20.0
BAND_GAP = 12.0

BAND_HEIGHT = (HEIGHT - 4 * BAND_GAP) / 5.0
BAND_WIDTH = WIDTH - COLUMN_WIDTH - COLUMN_GAP
COLUMN_X = ORIGIN_X + WIDTH - COLUMN_WIDTH
COLUMN_HEIGHT = 4 * BAND_HEIGHT + 3 * BAND_GAP
BOTTOM = ORIGIN_Y + HEIGHT

COLUMN_HEADER_FONT_SIZE = BODY_SIZE
PART_GAP = 8.0


@dataclass(frozen=True)
class Figure:
    layers: Mapping[int, dict]
    parts: Mapping[int, dict]

    def layer(self, number: int) -> dict:
        return self.layers[number]

    def part(self, number: int) -> dict:
        return self.parts[number]

    @property
    def bottom(self) -> float:
        return BOTTOM


def _tone_for(tones: Mapping[int, Tone] | None, number: int, fallback: Tone) -> Tone:
    if tones is None:
        return fallback
    return tones.get(number, fallback)


def _tag_for(tags: Mapping[int, str] | None, number: int) -> str:
    if tags is None:
        return ""
    return tags.get(number, "")


def draw(
    scene: Scene,
    *,
    layer_tones: Mapping[int, Tone] | None = None,
    layer_tags: Mapping[int, str] | None = None,
    part_tones: Mapping[int, Tone] | None = None,
    part_tags: Mapping[int, str] | None = None,
    default_layer_tone: Tone = NEUTRAL,
    default_part_tone: Tone = NEUTRAL,
    show_details: bool = True,
) -> Figure:
    drawn_layers: dict[int, dict] = {}
    for position, spec in enumerate(reversed(BAND_LAYERS)):
        spans_full_width = spec.number == 1
        drawn_layers[spec.number] = labelled_box(
            scene,
            ORIGIN_X,
            ORIGIN_Y + position * (BAND_HEIGHT + BAND_GAP),
            WIDTH if spans_full_width else BAND_WIDTH,
            BAND_HEIGHT,
            LabelledBox(
                title=spec.title,
                detail=spec.detail if show_details else "",
                tone=_tone_for(layer_tones, spec.number, default_layer_tone),
                tag=_tag_for(layer_tags, spec.number),
                index_label=str(spec.number),
            ),
            title_font_size=HEADING_SIZE,
            detail_font_size=BODY_SIZE,
            tag_font_size=CAPTION_SIZE,
            padding=16,
            index_gutter=46,
        )

    column_tone = _tone_for(layer_tones, ORCHESTRATION_LAYER.number, default_layer_tone)
    column = scene.rectangle(
        COLUMN_X, ORIGIN_Y, COLUMN_WIDTH, COLUMN_HEIGHT, column_tone
    )
    drawn_layers[ORCHESTRATION_LAYER.number] = column

    header = wrapped(
        f"{ORCHESTRATION_LAYER.number}. {ORCHESTRATION_LAYER.title}",
        COLUMN_WIDTH - 48,
        COLUMN_HEADER_FONT_SIZE,
    )
    header_height = measured_height(header, COLUMN_HEADER_FONT_SIZE)
    scene.text(
        COLUMN_X + 16,
        ORIGIN_Y + 12,
        header,
        font_size=COLUMN_HEADER_FONT_SIZE,
        colour=column_tone.text,
        align="center",
        width=COLUMN_WIDTH - 32,
    )

    cursor = ORIGIN_Y + 12 + header_height + 6
    column_tag = _tag_for(layer_tags, ORCHESTRATION_LAYER.number)
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

    parts_top = cursor + 4
    part_height = (
        ORIGIN_Y
        + COLUMN_HEIGHT
        - 12
        - parts_top
        - PART_GAP * (len(ORCHESTRATION_PARTS) - 1)
    ) / len(ORCHESTRATION_PARTS)

    drawn_parts: dict[int, dict] = {}
    for position, spec in enumerate(ORCHESTRATION_PARTS):
        drawn_parts[spec.number] = labelled_box(
            scene,
            COLUMN_X + 14,
            parts_top + position * (part_height + PART_GAP),
            COLUMN_WIDTH - 28,
            part_height,
            LabelledBox(
                title=spec.title,
                tone=_tone_for(part_tones, spec.number, default_part_tone),
                tag=_tag_for(part_tags, spec.number),
            ),
            title_font_size=BODY_SIZE,
            tag_font_size=CAPTION_SIZE,
            padding=12,
            align="center",
        )

    return Figure(layers=drawn_layers, parts=drawn_parts)
