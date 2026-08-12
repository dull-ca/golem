"""The offline half of verification: build every scene, then check what came out.

    python docs/presentation/test_scenes.py

Runs the real `build_all` into a temporary directory and asserts the file-format
invariants that are cheap to get wrong — required keys, ids that resolve both
ways, arrows anchored at their origin, labels that fit, and a rebuild that is
byte-identical. Several of these pin bugs that were live once; the comments below
say which. What this cannot check is whether Excalidraw agrees, because the schema
here is a restatement of the format rather than the format itself — `tools/` loads
the output through the real `restore()` for that, at the cost of a network install.
"""

from __future__ import annotations

import json
import math
import sys
import tempfile
import unittest
from pathlib import Path

PRESENTATION_ROOT = Path(__file__).resolve().parent
if str(PRESENTATION_ROOT) not in sys.path:
    sys.path.insert(0, str(PRESENTATION_ROOT))

from build import build_all
from excalidraw.scene import (
    BASE_KEYS,
    CANVAS_HEIGHT,
    CANVAS_WIDTH,
    CONTAINER_PADDING,
    LINEAR_KEYS,
    MARGIN,
    TEXT_KEYS,
    coordinate,
)
from excalidraw.text import MONO, measured_height, measured_width
from slides import SLIDES

LINEAR_TYPES = frozenset({"arrow", "line"})
DECK_FILENAME = "deck.excalidraw"

MARGIN_LEFT = MARGIN
MARGIN_TOP = MARGIN
MARGIN_RIGHT = CANVAS_WIDTH - MARGIN
MARGIN_BOTTOM = CANVAS_HEIGHT - MARGIN

# These three are Excalidraw's numbers, not the generator's, and that is the point:
# measuring the output against the estimates that produced it would prove nothing.
# TRUE_MONOSPACE_ADVANCE is the font's real advance, under text.MONOSPACE_ADVANCE;
# BOUND_TEXT_PADDING is the editor's real bound-text padding, well under
# scene.CONTAINER_PADDING. A mono label wrapped with hand-font metrics passed the
# generator's own arithmetic and still re-wrapped on load — hence these.
TRUE_MONOSPACE_ADVANCE = 0.62
BOUND_TEXT_PADDING = 5
MINIMUM_LABEL_SLACK = 8


def numbers_in(value) -> list[float]:
    if isinstance(value, bool):
        return []
    if isinstance(value, (int, float)):
        return [float(value)]
    if isinstance(value, dict):
        return [number for item in value.values() for number in numbers_in(item)]
    if isinstance(value, (list, tuple)):
        return [number for item in value for number in numbers_in(item)]
    return []


# An arrow's points are relative to its x,y and may run up or left, so x,y is a
# corner of the bbox only when they run down and right. Walk the points instead;
# `x + width` would put an upward arrow's top edge below where it is drawn and let
# it escape the canvas margin unnoticed.
def linear_extent(element: dict) -> tuple[float, float, float, float]:
    xs = [element["x"] + point[0] for point in element["points"]]
    ys = [element["y"] + point[1] for point in element["points"]]
    return min(xs), min(ys), max(xs), max(ys)


def extent(element: dict) -> tuple[float, float, float, float]:
    if element["type"] in LINEAR_TYPES:
        return linear_extent(element)
    return (
        element["x"],
        element["y"],
        element["x"] + element["width"],
        element["y"] + element["height"],
    )


def monospace_line_advance(line: str, font_size: float) -> float:
    return len(line) * TRUE_MONOSPACE_ADVANCE * font_size


class GeneratedScenes(unittest.TestCase):
    output: Path
    documents: dict[str, dict]

    @classmethod
    def setUpClass(cls) -> None:
        cls._workspace = tempfile.TemporaryDirectory()
        cls.output = Path(cls._workspace.name) / "first"
        build_all(cls.output)
        cls.documents = {
            path.name: json.loads(path.read_text(encoding="utf-8"))
            for path in sorted(cls.output.glob("*.excalidraw"))
        }

    @classmethod
    def tearDownClass(cls) -> None:
        cls._workspace.cleanup()

    def test_every_slide_and_the_deck_are_written(self) -> None:
        expected = {slide.filename for slide in SLIDES} | {"deck.excalidraw"}
        self.assertEqual(set(self.documents), expected)

    def test_documents_have_the_excalidraw_envelope(self) -> None:
        for name, payload in self.documents.items():
            with self.subTest(name):
                self.assertEqual(payload["type"], "excalidraw")
                self.assertEqual(payload["version"], 2)
                self.assertIsInstance(payload["elements"], list)
                self.assertTrue(payload["elements"])
                self.assertEqual(payload["files"], {})
                self.assertIn("viewBackgroundColor", payload["appState"])

    def test_elements_carry_every_required_key(self) -> None:
        for name, payload in self.documents.items():
            for element in payload["elements"]:
                with self.subTest(name=name, element=element["id"]):
                    for key in BASE_KEYS:
                        self.assertIn(key, element)
                    self.assertNotIn("index", element)
                    if element["type"] == "text":
                        for key in TEXT_KEYS:
                            self.assertIn(key, element)
                    if element["type"] in LINEAR_TYPES:
                        for key in LINEAR_KEYS:
                            self.assertIn(key, element)
                    if element["type"] == "frame":
                        self.assertIsInstance(element["name"], str)
                        self.assertIsNone(element["roundness"])

    def test_identifiers_are_unique(self) -> None:
        for name, payload in self.documents.items():
            with self.subTest(name):
                identifiers = [element["id"] for element in payload["elements"]]
                self.assertEqual(len(identifiers), len(set(identifiers)))

    def test_bound_text_pairs_resolve_both_ways(self) -> None:
        for name, payload in self.documents.items():
            elements = {element["id"]: element for element in payload["elements"]}
            for element in payload["elements"]:
                with self.subTest(name=name, element=element["id"]):
                    container_id = element.get("containerId")
                    if container_id is not None:
                        self.assertIn(container_id, elements)
                        container = elements[container_id]
                        self.assertIn(
                            {"type": "text", "id": element["id"]},
                            container["boundElements"],
                        )
                        self.assertEqual(element["text"], element["originalText"])
                    for bound in element["boundElements"] or ():
                        self.assertIn(bound["id"], elements)

    def test_frame_membership_resolves_to_a_frame(self) -> None:
        for name, payload in self.documents.items():
            elements = {element["id"]: element for element in payload["elements"]}
            frames_already_emitted: set[str] = set()
            for element in payload["elements"]:
                frame_id = element["frameId"]
                if frame_id is not None:
                    with self.subTest(name=name, element=element["id"]):
                        self.assertIn(frame_id, elements)
                        self.assertEqual(elements[frame_id]["type"], "frame")
                        self.assertIn(frame_id, frames_already_emitted)
                if element["type"] == "frame":
                    frames_already_emitted.add(element["id"])

    def test_linear_elements_anchor_at_the_origin(self) -> None:
        for name, payload in self.documents.items():
            for element in payload["elements"]:
                if element["type"] not in LINEAR_TYPES:
                    continue
                with self.subTest(name=name, element=element["id"]):
                    points = element["points"]
                    self.assertGreaterEqual(len(points), 2)
                    self.assertEqual(points[0], [0, 0])
                    xs = [point[0] for point in points]
                    ys = [point[1] for point in points]
                    self.assertEqual(
                        element["width"], coordinate(max(xs) - min(xs))
                    )
                    self.assertEqual(
                        element["height"], coordinate(max(ys) - min(ys))
                    )

    def test_no_non_finite_numbers(self) -> None:
        for name, payload in self.documents.items():
            with self.subTest(name):
                for number in numbers_in(payload["elements"]):
                    self.assertTrue(math.isfinite(number))

    def test_geometry_is_non_negative_in_size(self) -> None:
        for name, payload in self.documents.items():
            for element in payload["elements"]:
                with self.subTest(name=name, element=element["id"]):
                    self.assertGreaterEqual(element["width"], 0)
                    self.assertGreaterEqual(element["height"], 0)

    def test_fonts_are_hand_drawn_unless_code(self) -> None:
        for name, payload in self.documents.items():
            for element in payload["elements"]:
                if element["type"] != "text":
                    continue
                with self.subTest(name=name, element=element["id"]):
                    self.assertIn(element["fontFamily"], (1, 3))

    def test_deck_holds_one_frame_per_slide(self) -> None:
        deck = self.documents["deck.excalidraw"]
        frames = [
            element for element in deck["elements"] if element["type"] == "frame"
        ]
        self.assertEqual(len(frames), len(SLIDES))
        self.assertEqual(
            [frame["name"] for frame in frames],
            [slide.frame_name for slide in SLIDES],
        )
        for element in deck["elements"]:
            if element["type"] != "frame":
                self.assertIsNotNone(element["frameId"])

    def test_slide_elements_stay_inside_the_canvas_margin(self) -> None:
        for name, payload in self.documents.items():
            if name == DECK_FILENAME:
                continue
            for element in payload["elements"]:
                left, top, right, bottom = extent(element)
                with self.subTest(name=name, element=element["id"]):
                    self.assertGreaterEqual(left, MARGIN_LEFT)
                    self.assertGreaterEqual(top, MARGIN_TOP)
                    self.assertLessEqual(right, MARGIN_RIGHT)
                    self.assertLessEqual(bottom, MARGIN_BOTTOM)

    def test_bound_labels_fit_inside_their_container(self) -> None:
        for name, payload in self.documents.items():
            elements = {element["id"]: element for element in payload["elements"]}
            for element in payload["elements"]:
                container_id = element.get("containerId")
                if container_id is None:
                    continue
                container = elements[container_id]
                with self.subTest(name=name, element=element["id"]):
                    self.assertLessEqual(
                        measured_width(element["text"], element["fontSize"]),
                        container["width"] - 2 * CONTAINER_PADDING,
                    )
                    self.assertLessEqual(
                        measured_height(element["text"], element["fontSize"]),
                        container["height"],
                    )

    def test_unbound_monospace_text_fits_its_declared_box(self) -> None:
        for name, payload in self.documents.items():
            for element in payload["elements"]:
                if element["type"] != "text" or element["fontFamily"] != MONO:
                    continue
                if element["containerId"] is not None:
                    continue
                for line in element["text"].split("\n"):
                    with self.subTest(name=name, element=element["id"], line=line):
                        self.assertLessEqual(
                            monospace_line_advance(line, element["fontSize"]),
                            element["width"],
                        )

    def test_bound_monospace_labels_clear_the_real_bound_text_padding(self) -> None:
        for name, payload in self.documents.items():
            elements = {element["id"]: element for element in payload["elements"]}
            for element in payload["elements"]:
                if element["type"] != "text" or element["fontFamily"] != MONO:
                    continue
                container_id = element["containerId"]
                if container_id is None:
                    continue
                container = elements[container_id]
                room = (
                    container["width"] - 2 * BOUND_TEXT_PADDING - MINIMUM_LABEL_SLACK
                )
                for line in element["text"].split("\n"):
                    with self.subTest(name=name, element=element["id"], line=line):
                        self.assertLessEqual(
                            monospace_line_advance(line, element["fontSize"]), room
                        )

    def test_the_build_is_byte_identical_when_repeated(self) -> None:
        with tempfile.TemporaryDirectory() as workspace:
            repeat = Path(workspace) / "second"
            build_all(repeat)
            first = sorted(self.output.glob("*.excalidraw"))
            second = sorted(repeat.glob("*.excalidraw"))
            self.assertEqual(
                [path.name for path in first], [path.name for path in second]
            )
            for original, rebuilt in zip(first, second):
                with self.subTest(original.name):
                    self.assertEqual(original.read_bytes(), rebuilt.read_bytes())


if __name__ == "__main__":
    unittest.main()
