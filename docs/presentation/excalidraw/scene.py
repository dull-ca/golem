"""Excalidraw elements as plain dicts, and the file they serialise into.

Every element carries the whole key set the format expects — `BASE_KEYS`, plus
`TEXT_KEYS` or `LINEAR_KEYS` for those shapes. The factories on `Scene` fill
those in and append to `Scene.elements`, which is also the z-order: Excalidraw
rebuilds stacking from array order, so append back-to-front. `framed_deck` copies
finished scenes into one canvas, each inside an Excalidraw frame.

Ids, seeds and nonces derive from `blake2s(scene key + counter)` and `updated` is
a constant, so a rebuild of the same scene is byte-identical to the last one.
"""

from __future__ import annotations

import copy
import hashlib
import json
from pathlib import Path
from typing import Iterable, Sequence

from .palette import INK, TRANSPARENT, Tone
from .text import HAND, LINE_HEIGHT, measured_height, measured_width, wrapped

CANVAS_WIDTH = 1600
CANVAS_HEIGHT = 1000
MARGIN = 64
CONTENT_WIDTH = CANVAS_WIDTH - 2 * MARGIN
CONTENT_HEIGHT = CANVAS_HEIGHT - 2 * MARGIN
CONTENT_LEFT = MARGIN
CONTENT_RIGHT = CANVAS_WIDTH - MARGIN

SOURCE = "golem docs/presentation"
UPDATED = 1735689600000
VERSION = 1

# NOTE: Excalidraw's real bound-text padding is 5px. This is deliberately wider —
# the surplus is slack for the width estimate in text.py, not a match for the editor.
CONTAINER_PADDING = 12

# NOTE: measured widths are estimates, and the browser's real font metrics can run
# wider. Labels are wrapped to this fraction of the space they have so that the
# overshoot lands in the reserve instead of overflowing the shape on load.
LABEL_HEADROOM = 0.88

ROUNDED = {"type": 3}
CURVED = {"type": 2}
SHARP = None

BASE_KEYS = (
    "id",
    "type",
    "x",
    "y",
    "width",
    "height",
    "angle",
    "strokeColor",
    "backgroundColor",
    "fillStyle",
    "strokeWidth",
    "strokeStyle",
    "roughness",
    "opacity",
    "groupIds",
    "frameId",
    "roundness",
    "seed",
    "version",
    "versionNonce",
    "isDeleted",
    "boundElements",
    "updated",
    "link",
    "locked",
)

TEXT_KEYS = (
    "text",
    "fontSize",
    "fontFamily",
    "textAlign",
    "verticalAlign",
    "containerId",
    "originalText",
    "lineHeight",
    "autoResize",
)

LINEAR_KEYS = (
    "points",
    "lastCommittedPoint",
    "startBinding",
    "endBinding",
    "startArrowhead",
    "endArrowhead",
    "elbowed",
)


def coordinate(value: float) -> float:
    return round(float(value), 2)


def bounds(element: dict) -> tuple[float, float, float, float]:
    return (element["x"], element["y"], element["width"], element["height"])


def right_edge(element: dict) -> float:
    return element["x"] + element["width"]


def bottom_edge(element: dict) -> float:
    return element["y"] + element["height"]


def centre(element: dict) -> tuple[float, float]:
    return (
        element["x"] + element["width"] / 2.0,
        element["y"] + element["height"] / 2.0,
    )


def label_wrap_width(container_width: float) -> float:
    return (container_width - 2 * CONTAINER_PADDING) * LABEL_HEADROOM


def fit_width(
    body: str,
    font_size: float,
    padding: float = CONTAINER_PADDING,
    font_family: int = HAND,
) -> float:
    return (
        measured_width(body, font_size, font_family) / LABEL_HEADROOM
        + 2 * padding
        + 2
    )


class Scene:
    def __init__(
        self,
        key: str,
        *,
        width: int = CANVAS_WIDTH,
        height: int = CANVAS_HEIGHT,
        background: str = "#ffffff",
    ) -> None:
        self.key = key
        self.width = width
        self.height = height
        self.background = background
        self.elements: list[dict] = []
        self._counter = 0

    def _identity(self) -> tuple[str, int, int]:
        self._counter += 1
        stem = f"{self.key}#{self._counter}".encode("utf-8")
        identifier = hashlib.blake2s(stem, digest_size=12).hexdigest()
        seed = int.from_bytes(hashlib.blake2s(stem + b"/seed", digest_size=4).digest(), "big")
        nonce = int.from_bytes(hashlib.blake2s(stem + b"/nonce", digest_size=4).digest(), "big")
        return identifier, seed % 2_000_000_000, nonce % 2_000_000_000

    def _base(
        self,
        kind: str,
        x: float,
        y: float,
        width: float,
        height: float,
        *,
        stroke: str,
        background: str,
        fill_style: str,
        stroke_width: float,
        stroke_style: str,
        roughness: int,
        opacity: int,
        roundness: dict | None,
    ) -> dict:
        identifier, seed, nonce = self._identity()
        # NOTE: no `index` key. Excalidraw's restore() regenerates the fractional
        # index from array order; a hand-rolled one that is not strictly increasing
        # corrupts z-order instead of setting it. test_scenes.py asserts its absence.
        return {
            "id": identifier,
            "type": kind,
            "x": coordinate(x),
            "y": coordinate(y),
            "width": coordinate(width),
            "height": coordinate(height),
            "angle": 0,
            "strokeColor": stroke,
            "backgroundColor": background,
            "fillStyle": fill_style,
            "strokeWidth": stroke_width,
            "strokeStyle": stroke_style,
            "roughness": roughness,
            "opacity": opacity,
            "groupIds": [],
            "frameId": None,
            "roundness": roundness,
            "seed": seed,
            "version": VERSION,
            "versionNonce": nonce,
            "isDeleted": False,
            "boundElements": [],
            "updated": UPDATED,
            "link": None,
            "locked": False,
        }

    def _shape(
        self,
        kind: str,
        x: float,
        y: float,
        width: float,
        height: float,
        tone: Tone,
        *,
        radius: bool,
        stroke_width: float,
        stroke_style: str,
        fill_style: str,
        roughness: int,
        opacity: int,
        label: str,
        label_font_size: float,
        label_font_family: int,
        label_colour: str | None,
        label_align: str,
        label_wrap: bool,
    ) -> dict:
        element = self._base(
            kind,
            x,
            y,
            width,
            height,
            stroke=tone.stroke,
            background=tone.fill,
            fill_style=fill_style,
            stroke_width=stroke_width,
            stroke_style=stroke_style,
            roughness=roughness,
            opacity=opacity,
            roundness=ROUNDED if radius else SHARP,
        )
        self.elements.append(element)
        if label:
            self.bind_label(
                element,
                label,
                font_size=label_font_size,
                font_family=label_font_family,
                colour=label_colour if label_colour is not None else tone.text,
                align=label_align,
                wrap=label_wrap,
            )
        return element

    def rectangle(
        self,
        x: float,
        y: float,
        width: float,
        height: float,
        tone: Tone,
        *,
        radius: bool = True,
        stroke_width: float = 2,
        stroke_style: str = "solid",
        fill_style: str = "solid",
        roughness: int = 1,
        opacity: int = 100,
        label: str = "",
        label_font_size: float = 16,
        label_font_family: int = HAND,
        label_colour: str | None = None,
        label_align: str = "center",
        label_wrap: bool = True,
    ) -> dict:
        return self._shape(
            "rectangle",
            x,
            y,
            width,
            height,
            tone,
            radius=radius,
            stroke_width=stroke_width,
            stroke_style=stroke_style,
            fill_style=fill_style,
            roughness=roughness,
            opacity=opacity,
            label=label,
            label_font_size=label_font_size,
            label_font_family=label_font_family,
            label_colour=label_colour,
            label_align=label_align,
            label_wrap=label_wrap,
        )

    def ellipse(
        self,
        x: float,
        y: float,
        width: float,
        height: float,
        tone: Tone,
        *,
        stroke_width: float = 2,
        stroke_style: str = "solid",
        fill_style: str = "solid",
        roughness: int = 1,
        opacity: int = 100,
        label: str = "",
        label_font_size: float = 16,
        label_font_family: int = HAND,
        label_colour: str | None = None,
        label_align: str = "center",
        label_wrap: bool = True,
    ) -> dict:
        return self._shape(
            "ellipse",
            x,
            y,
            width,
            height,
            tone,
            radius=True,
            stroke_width=stroke_width,
            stroke_style=stroke_style,
            fill_style=fill_style,
            roughness=roughness,
            opacity=opacity,
            label=label,
            label_font_size=label_font_size,
            label_font_family=label_font_family,
            label_colour=label_colour,
            label_align=label_align,
            label_wrap=label_wrap,
        )

    def diamond(
        self,
        x: float,
        y: float,
        width: float,
        height: float,
        tone: Tone,
        *,
        stroke_width: float = 2,
        stroke_style: str = "solid",
        fill_style: str = "solid",
        roughness: int = 1,
        opacity: int = 100,
        label: str = "",
        label_font_size: float = 16,
        label_font_family: int = HAND,
        label_colour: str | None = None,
        label_align: str = "center",
        label_wrap: bool = True,
    ) -> dict:
        return self._shape(
            "diamond",
            x,
            y,
            width,
            height,
            tone,
            radius=True,
            stroke_width=stroke_width,
            stroke_style=stroke_style,
            fill_style=fill_style,
            roughness=roughness,
            opacity=opacity,
            label=label,
            label_font_size=label_font_size,
            label_font_family=label_font_family,
            label_colour=label_colour,
            label_align=label_align,
            label_wrap=label_wrap,
        )

    def frame(self, x: float, y: float, width: float, height: float, name: str) -> dict:
        element = self._base(
            "frame",
            x,
            y,
            width,
            height,
            stroke="#bbb",
            background=TRANSPARENT,
            fill_style="solid",
            stroke_width=2,
            stroke_style="solid",
            roughness=0,
            opacity=100,
            roundness=SHARP,
        )
        element["name"] = name
        self.elements.append(element)
        return element

    def text(
        self,
        x: float,
        y: float,
        body: str,
        *,
        font_size: float = 16,
        colour: str = INK,
        font_family: int = HAND,
        align: str = "left",
        vertical_align: str = "top",
        width: float | None = None,
        wrap_width: float | None = None,
        opacity: int = 100,
    ) -> dict:
        if wrap_width is not None:
            body = wrapped(body, wrap_width * LABEL_HEADROOM, font_size)
        box_width = width if width is not None else measured_width(body, font_size)
        box_height = measured_height(body, font_size)
        element = self._base(
            "text",
            x,
            y,
            box_width,
            box_height,
            stroke=colour,
            background=TRANSPARENT,
            fill_style="solid",
            stroke_width=2,
            stroke_style="solid",
            roughness=1,
            opacity=opacity,
            roundness=SHARP,
        )
        element.update(
            {
                "text": body,
                "fontSize": font_size,
                "fontFamily": font_family,
                "textAlign": align,
                "verticalAlign": vertical_align,
                "containerId": None,
                "originalText": body,
                "lineHeight": LINE_HEIGHT,
                "autoResize": True,
            }
        )
        self.elements.append(element)
        return element

    def bind_label(
        self,
        container: dict,
        body: str,
        *,
        font_size: float = 16,
        colour: str = INK,
        font_family: int = HAND,
        align: str = "center",
        vertical_align: str = "middle",
        wrap: bool = True,
    ) -> dict:
        # NOTE: font_family has to reach every measurement below. Measuring a mono
        # label with the hand-font table under-measures it, and Excalidraw then
        # re-wraps the code literal on load, breaking the layout computed here.
        laid_out = (
            wrapped(
                body, label_wrap_width(container["width"]), font_size, font_family
            )
            if wrap
            else body
        )
        box_width = min(
            measured_width(laid_out, font_size, font_family),
            container["width"] - 2 * CONTAINER_PADDING,
        )
        box_height = measured_height(laid_out, font_size, font_family)
        element = self._base(
            "text",
            container["x"] + (container["width"] - box_width) / 2.0,
            container["y"] + (container["height"] - box_height) / 2.0,
            box_width,
            box_height,
            stroke=colour,
            background=TRANSPARENT,
            fill_style="solid",
            stroke_width=2,
            stroke_style="solid",
            roughness=1,
            opacity=container["opacity"],
            roundness=SHARP,
        )
        element.update(
            {
                "text": laid_out,
                "fontSize": font_size,
                "fontFamily": font_family,
                "textAlign": align,
                "verticalAlign": vertical_align,
                "containerId": container["id"],
                "originalText": laid_out,
                "lineHeight": LINE_HEIGHT,
                "autoResize": False,
            }
        )
        container["boundElements"].append({"type": "text", "id": element["id"]})
        self.elements.append(element)
        return element

    def _linear(
        self,
        kind: str,
        points: Sequence[tuple[float, float]],
        *,
        stroke: str,
        stroke_width: float,
        stroke_style: str,
        roughness: int,
        opacity: int,
        start_arrowhead: str | None,
        end_arrowhead: str | None,
    ) -> dict:
        # NOTE: the caller passes absolute points; the format stores them relative to
        # the element's x,y, with points[0] == [0,0]. width/height are therefore the
        # span of the points, not an offset from x,y — for an arrow travelling up or
        # left the visual bbox reaches *back* from the anchor, and `x + width` is not
        # its right edge. Derive a linear element's bbox from the points.
        anchor_x, anchor_y = points[0]
        relative = [
            [coordinate(x - anchor_x), coordinate(y - anchor_y)] for x, y in points
        ]
        xs = [point[0] for point in relative]
        ys = [point[1] for point in relative]
        element = self._base(
            kind,
            anchor_x,
            anchor_y,
            max(xs) - min(xs),
            max(ys) - min(ys),
            stroke=stroke,
            background=TRANSPARENT,
            fill_style="solid",
            stroke_width=stroke_width,
            stroke_style=stroke_style,
            roughness=roughness,
            opacity=opacity,
            roundness=CURVED,
        )
        element.update(
            {
                "points": relative,
                "lastCommittedPoint": None,
                "startBinding": None,
                "endBinding": None,
                "startArrowhead": start_arrowhead,
                "endArrowhead": end_arrowhead,
                "elbowed": False,
            }
        )
        self.elements.append(element)
        return element

    def arrow(
        self,
        points: Sequence[tuple[float, float]],
        *,
        stroke: str = INK,
        stroke_width: float = 2,
        stroke_style: str = "solid",
        roughness: int = 1,
        opacity: int = 100,
        start_arrowhead: str | None = None,
        end_arrowhead: str | None = "arrow",
    ) -> dict:
        return self._linear(
            "arrow",
            points,
            stroke=stroke,
            stroke_width=stroke_width,
            stroke_style=stroke_style,
            roughness=roughness,
            opacity=opacity,
            start_arrowhead=start_arrowhead,
            end_arrowhead=end_arrowhead,
        )

    def line(
        self,
        points: Sequence[tuple[float, float]],
        *,
        stroke: str = INK,
        stroke_width: float = 2,
        stroke_style: str = "solid",
        roughness: int = 1,
        opacity: int = 100,
    ) -> dict:
        return self._linear(
            "line",
            points,
            stroke=stroke,
            stroke_width=stroke_width,
            stroke_style=stroke_style,
            roughness=roughness,
            opacity=opacity,
            start_arrowhead=None,
            end_arrowhead=None,
        )


def reframed(
    elements: Iterable[dict], offset_x: float, offset_y: float, frame_id: str
) -> list[dict]:
    moved: list[dict] = []
    for element in elements:
        clone = copy.deepcopy(element)
        clone["x"] = coordinate(clone["x"] + offset_x)
        clone["y"] = coordinate(clone["y"] + offset_y)
        clone["frameId"] = frame_id
        moved.append(clone)
    return moved


def framed_deck(
    named_scenes: Sequence[tuple[str, Scene]],
    *,
    key: str = "deck",
    columns: int = 3,
    gap: float = 160.0,
) -> Scene:
    deck = Scene(key)
    for position, (name, scene) in enumerate(named_scenes):
        offset_x = (position % columns) * (scene.width + gap)
        offset_y = (position // columns) * (scene.height + gap)
        frame = deck.frame(offset_x, offset_y, scene.width, scene.height, name)
        deck.elements.extend(reframed(scene.elements, offset_x, offset_y, frame["id"]))
    if named_scenes:
        deck.width = min(columns, len(named_scenes)) * (CANVAS_WIDTH + gap)
        deck.height = ((len(named_scenes) - 1) // columns + 1) * (CANVAS_HEIGHT + gap)
    return deck


def document(scene: Scene) -> dict:
    return {
        "type": "excalidraw",
        "version": 2,
        "source": SOURCE,
        "elements": scene.elements,
        "appState": {
            "gridSize": None,
            "gridStep": 5,
            "gridModeEnabled": False,
            "viewBackgroundColor": scene.background,
        },
        "files": {},
    }


def serialised(scene: Scene) -> str:
    return json.dumps(document(scene), indent=2, ensure_ascii=False) + "\n"


def write_scene(path: Path, scene: Scene) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(serialised(scene), encoding="utf-8")
    return path
