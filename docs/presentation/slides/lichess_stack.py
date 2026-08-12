"""The six-layer lichess figure, drawn once and recoloured by four slides.

Slides 03, 05, 06 and 07 make the same argument from the same shape: here is
everything a host has to answer, now watch who answers it. `draw()` therefore
takes per-layer and per-part `Tone`s and tags rather than owning any colour, so
those slides recolour the figure instead of redrawing it — a band that moves
moves on all four. The returned `Figure` exposes the boxes so a caller can hang
callouts and gap marks off the exact layer or orchestration part it means.

Layer 6 is a column beside bands 2 to 5, not a band above them, because
orchestration acts across those layers rather than sitting on top of them. Layer
1 is the only band drawn full width — it runs under the column too.
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


class LayerSpec(NamedTuple):
    number: int
    title: str
    detail: str


BAND_LAYERS: tuple[LayerSpec, ...] = (
    LayerSpec(
        1,
        "Core OS, network, security",
        "Debian, kernel, users, sshd, nftables, the private network, TLS",
    ),
    LayerSpec(
        2,
        "Application hosting",
        "container runtime (podman), systemd, storage and volumes, registry access",
    ),
    LayerSpec(
        3,
        "Connective infrastructure",
        "DNS, SRV records, service discovery, reverse proxies, load balancers, the private network fabric",
    ),
    LayerSpec(
        4,
        "Tools, dependencies, runtimes",
        "JVM, node, native libs, client libraries, base images",
    ),
    LayerSpec(
        5,
        "The applications",
        "lila, lila-ws, lila-search, mongodb, redis, the rest",
    ),
)

ORCHESTRATION_LAYER = LayerSpec(
    6,
    "Lifecycle / schedule / scaling",
    "orchestration acts across layers 2 to 5, it does not sit on top of them",
)

ORCHESTRATION_PARTS: tuple[LayerSpec, ...] = (
    LayerSpec(1, "Placement", "the scheduler — the only part that answers which node"),
    LayerSpec(
        2,
        "Lifecycle",
        "starting, stopping, restarting, draining, rolling updates, rollbacks",
    ),
    LayerSpec(
        3,
        "Health and reconciliation",
        "watching actual state, detecting drift or failure, rescheduling",
    ),
    LayerSpec(
        4,
        "Supporting plumbing",
        "networking, service discovery, load balancer registration, storage, secrets",
    ),
    LayerSpec(5, "Scaling", "adjusting replica counts in response to policy or load"),
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

DEFAULT_ORIGIN_X = MARGIN
DEFAULT_ORIGIN_Y = 192
DEFAULT_WIDTH = CONTENT_WIDTH
DEFAULT_HEIGHT = 648
DEFAULT_COLUMN_WIDTH = 384
DEFAULT_COLUMN_GAP = 18
DEFAULT_BAND_GAP = 12


@dataclass(frozen=True)
class Figure:
    x: float
    y: float
    width: float
    height: float
    band_width: float
    column_x: float
    column_width: float
    column_height: float
    layers: Mapping[int, dict]
    parts: Mapping[int, dict]

    def layer(self, number: int) -> dict:
        return self.layers[number]

    def part(self, number: int) -> dict:
        return self.parts[number]

    @property
    def right(self) -> float:
        return self.x + self.width

    @property
    def bottom(self) -> float:
        return self.y + self.height


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
    x: float = DEFAULT_ORIGIN_X,
    y: float = DEFAULT_ORIGIN_Y,
    width: float = DEFAULT_WIDTH,
    height: float = DEFAULT_HEIGHT,
    layer_tones: Mapping[int, Tone] | None = None,
    layer_tags: Mapping[int, str] | None = None,
    part_tones: Mapping[int, Tone] | None = None,
    part_tags: Mapping[int, str] | None = None,
    default_layer_tone: Tone = NEUTRAL,
    default_part_tone: Tone = NEUTRAL,
    column_width: float = DEFAULT_COLUMN_WIDTH,
    column_gap: float = DEFAULT_COLUMN_GAP,
    band_gap: float = DEFAULT_BAND_GAP,
    show_details: bool = True,
) -> Figure:
    band_height = (height - 4 * band_gap) / 5.0
    band_width = width - column_width - column_gap
    column_x = x + width - column_width
    column_height = 4 * band_height + 3 * band_gap

    drawn_layers: dict[int, dict] = {}
    for position, spec in enumerate(reversed(BAND_LAYERS)):
        spans_full_width = spec.number == 1
        drawn_layers[spec.number] = labelled_box(
            scene,
            x,
            y + position * (band_height + band_gap),
            width if spans_full_width else band_width,
            band_height,
            LabelledBox(
                title=spec.title,
                detail=spec.detail if show_details else "",
                tone=_tone_for(layer_tones, spec.number, default_layer_tone),
                tag=_tag_for(layer_tags, spec.number),
                index_label=str(spec.number),
            ),
            title_font_size=20,
            detail_font_size=13,
            tag_font_size=13,
            padding=14,
            index_gutter=34,
        )

    column_tone = _tone_for(layer_tones, ORCHESTRATION_LAYER.number, default_layer_tone)
    column = scene.rectangle(column_x, y, column_width, column_height, column_tone)
    drawn_layers[ORCHESTRATION_LAYER.number] = column

    header_font_size = 17
    header = wrapped(
        f"{ORCHESTRATION_LAYER.number}. {ORCHESTRATION_LAYER.title}",
        column_width - 44,
        header_font_size,
    )
    header_height = measured_height(header, header_font_size)
    scene.text(
        column_x + 14,
        y + 12,
        header,
        font_size=header_font_size,
        colour=column_tone.text,
        align="center",
        width=column_width - 28,
    )

    cursor = y + 12 + header_height + 6
    column_tag = _tag_for(layer_tags, ORCHESTRATION_LAYER.number)
    if column_tag:
        chip = badge(
            scene,
            column_x + column_width / 2.0,
            cursor,
            column_tag,
            tone=Tone(column_tone.stroke, WHITE, column_tone.stroke),
            font_size=13,
            anchor="center",
        )
        cursor += chip["height"] + 6

    part_gap = 8
    parts_top = cursor + 4
    part_height = (
        y + column_height - 12 - parts_top - part_gap * (len(ORCHESTRATION_PARTS) - 1)
    ) / len(ORCHESTRATION_PARTS)

    drawn_parts: dict[int, dict] = {}
    for position, spec in enumerate(ORCHESTRATION_PARTS):
        drawn_parts[spec.number] = labelled_box(
            scene,
            column_x + 12,
            parts_top + position * (part_height + part_gap),
            column_width - 24,
            part_height,
            LabelledBox(
                title=spec.title,
                detail=spec.detail if show_details else "",
                tone=_tone_for(part_tones, spec.number, default_part_tone),
                tag=_tag_for(part_tags, spec.number),
            ),
            title_font_size=14,
            detail_font_size=11,
            tag_font_size=11,
            padding=9,
        )

    return Figure(
        x=x,
        y=y,
        width=width,
        height=height,
        band_width=band_width,
        column_x=column_x,
        column_width=column_width,
        column_height=column_height,
        layers=drawn_layers,
        parts=drawn_parts,
    )
