"""The offline half of verification: build every scene, then check what came out.

    python docs/presentation/test_scenes.py

Runs the real `build_all` into a temporary directory twice and asserts what is
cheap to get wrong — required keys, ids that resolve both ways, arrows anchored
at their origin, labels that fit, no text under the type floor, a word budget
per slide, icons that stay inside the box they declare, and two independent
builds that come out byte for byte identical. Several of these pin bugs that
were live once; the comments below say which.

What this cannot check is whether Excalidraw agrees, because the schema here is
a restatement of the format rather than the format itself — `tools/` loads the
output through the real `restore()` for that, at the cost of a network install.
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
from decks import DECKS
from decks.golem import enactment, glyph_ops, goals, scorecard
from decks.golem import (
    s16_the_goals,
    s19_plan_the_first_apply,
    s20_after_the_first_apply,
    s21_plan_one_host_changes,
    s22_emptying_a_host,
    s34_grading_goals_one_to_three,
    s35_grading_goals_four_and_five,
    s39_the_diff,
)
from excalidraw import icons
from excalidraw.scene import (
    BASE_KEYS,
    CANVAS_HEIGHT,
    CANVAS_WIDTH,
    CONTAINER_PADDING,
    FILE_KEYS,
    IMAGE_KEYS,
    LINEAR_KEYS,
    MARGIN,
    TEXT_KEYS,
    UPDATED,
    Scene,
)
from excalidraw.layout import StateNode, Transition, state_machine
from excalidraw.palette import NEUTRAL
from excalidraw.text import MONO, measured_height, measured_width
from excalidraw.type_scale import MINIMUM_SIZE, TITLE_SIZE, WORDS_PER_SLIDE
from icon_sheet import ICON_SHEET_FILENAME

LINEAR_TYPES = frozenset({"arrow", "line"})

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

GOLEM_DECK_NAME = "golem"
SCORECARD_SLUGS = (
    s34_grading_goals_one_to_three.SLUG,
    s35_grading_goals_four_and_five.SLUG,
)

ENACTMENT_MODULES = (
    s19_plan_the_first_apply,
    s20_after_the_first_apply,
    s21_plan_one_host_changes,
    s22_emptying_a_host,
)
ENACTMENT_SLUGS = tuple(module.SLUG for module in ENACTMENT_MODULES)
PLANNED_THEN_APPLIED = (s19_plan_the_first_apply, s20_after_the_first_apply)

ICON_PROBE_ORIGIN = (400.0, 300.0)
ICON_PROBE_SIZE = 96.0
GEOMETRY_TOLERANCE = 0.5

# A ceiling here is an exemption from WORDS_PER_SLIDE, and a ratchet: the slide is
# already over the default budget for the stated reason, and cannot get wordier
# without someone raising the number on purpose. Slides not listed hold the
# default. Every key must name a real slide — test_word_budget_ceilings_name_a_real
# _slide keeps this table from outliving the slides it excuses.
WORD_BUDGET_CEILINGS: dict[str, tuple[int, str]] = {
    "golem/what-you-buy": (60, "a matrix of fourteen labels, plus what the OS row means"),
    "golem/what-you-configure": (56, "eight row labels, six column headers, a marker, and a fourth legend entry for the cluster-membership X"),
    "golem/where-lichess-sits": (44, "six rungs of a ladder, and the marker on one"),
    "golem/where-lichess-sits-with-portainer": (44, "the ladder, plus Portainer stated at its real scale"),
    "golem/lichess-stack": (73, "the figure enumerates six layers and five parts"),
    "golem/orchestration": (67, "five named jobs, each with its own one-line gloss"),
    "golem/bought-orchestration": (78, "the same figure, plus one owner tag per layer"),
    "golem/ansible": (44, "six layer names and a coverage tag on every bar"),
    "golem/december-owners": (36, "five owner cards, each a name and what it owned"),
    "golem/december-placement": (39, "four stage cards, each a name and one gloss"),
    "golem/december-moving-a-service": (46, "four hand-ordered steps, each with a gloss"),
    "golem/fleet-machines": (86, "thirty real host names, the work key and the unit legend"),
    "golem/fleet-basics": (93, "thirty host names, the work key, the legend and one tool"),
    "golem/fleet-by-hand": (116, "thirty host names, the work key, the legend and two tools"),
    "golem/fleet-generated-config": (126, "thirty host names, the legend, and three tools named"),
    "golem/fleet-where-lichess-is-now": (131, "thirty host names, the legend, three tools and the counts"),
    "golem/playbook-a-file": (87, "thirty host names, what a cell means here, why the fleet is drawn bare, four play steps and what a step names"),
    "golem/playbook-a-line": (72, "thirty host names, what a cell means here, why the fleet is drawn bare, and four play steps"),
    "golem/playbook-more-files": (72, "thirty host names, what a cell means here, why the fleet is drawn bare, and four play steps"),
    "golem/playbook-a-workload": (81, "thirty host names, what a cell means here, why the fleet is drawn bare, four play steps and the two repeated hosts"),
    "golem/what-do-we-undo": (92, "thirty host names, what a mark is here, the count the play added, and what a playbook does not record"),
    "golem/golem-scrolls-compiled": (51, "a source tree quoted, and eight scrolls named"),
    "golem/golem-scrolls-dispatched": (50, "four hosts named twice, and what golemd does with a scroll"),
    "golem/plan-a-change": (84, "three hosts named twice, two operation rows each, three journals, and a legend of four operations and two cell states"),
    "golem/the-change-applied": (87, "the same figure with the plan enacted, and a third revision in every journal"),
    "golem/plan-one-host-changes": (93, "the same figure, plus the extra operation row on the host that changes and the sentence saying the plan was not applied"),
    "golem/emptying-a-host": (110, "the same figure with four revisions in every journal, the pointer from the record to the cells, and the two sentences that keep 'empty again' honest"),
    "golem/fleet-assembling": (121, "thirty host names, the legend, and three tools named"),
    "golem/fleet-golem": (136, "thirty host names, the legend, three tools and the icon credit"),
    "golem/moving-a-service": (62, "two hosts named three times each, and the limitation stated"),
    "golem/where-it-broke": (88, "five numbered problems, each a heading and one line"),
    "golem/what-golem-is": (53, "two claims stated as sentences, on purpose"),
    "golem/requirement-and-property": (88, "seven requirement and property pairs"),
    "golem/the-pipeline": (62, "five stage names, plus the manifest quoted verbatim"),
    "golem/the-diff": (53, "quoted Rust signatures and the four operation names"),
    "golem/apply-and-undo": (56, "quoted Rust signatures, one gloss each"),
    "golem/the-scroll-tree": (77, "the tree labels, two callouts, and the language Emet resembles"),
    "golem/the-four-glyphs": (98, "the whole glyph contract, quoted verbatim"),
    "golem/golemctl-verbs": (61, "quoted command lines, flags and a handshake"),
    "golem/golemd-routes": (87, "eight routes, each with a one-line gloss"),
    "golem/plan-against-host": (77, "quoted routes, types and Observation variants"),
    "golem/current-status": (75, "three status facts, plus the host scroll and the site's three glyphs quoted verbatim, and the two lines that keep the claim from reaching past what the code does"),
    "golem/longer-term-goals": (95, "the four glyph spellings, what golem's libraries already type on top of them, and three limits the language has today, each stated against the code"),
    "golem/adversarial-conditions": (138, "ten failure cases from the reconcilers, split three ways, each group with its result variant and what golem does with it"),
    "golem/grading-goals-1-to-3": (131, "four graded claims, each with its goal number, its verdict word and the evidence behind it, plus goal 1's three separate defects -- two of them named twice over, once on a chip and once in the sentence, so the row reads cold"),
    "golem/grading-goals-4-and-5": (77, "three graded claims, each with its goal number, its verdict word and the evidence behind it"),
    "orchestration/a-process-on-a-host": (48, "two lists of what a process gets and shares"),
    "orchestration/what-a-container-adds": (47, "three additions, each a name and one gloss"),
    "orchestration/the-image": (42, "three properties of an image, one line each"),
    "orchestration/registry-pull-run": (45, "four stage cards, each a name and one gloss"),
    "orchestration/one-host-many-containers": (48, "the runtime's jobs, one line each"),
    "orchestration/many-hosts-the-cluster": (48, "three cluster parts and the nodes they name"),
    "orchestration/the-five-jobs": (63, "five named jobs, each with its own one-line gloss"),
    "orchestration/what-lichess-uses": (42, "the five job names, the answer to each, the row outside them, and the legend"),
    "orchestration/the-december-plan": (54, "the same six rows, plus the two labels that split placement into a decision and its enactment"),
    "orchestration/with-golem": (62, "the same six rows, plus the four labels that split placement and lifecycle into decisions and their enactments"),
    "orchestration/placement-the-binding": (42, "three moments of the binding, named"),
    "orchestration/placement-what-it-weighs": (54, "four scheduler inputs, one line each"),
    "orchestration/lifecycle": (48, "five states, their transitions, and two named moves"),
    "orchestration/health-and-reconciliation": (50, "four stages of a loop, one line each"),
    "orchestration/connectivity-the-service": (50, "three stage cards, each a name and one gloss"),
    "orchestration/storage-and-secrets": (38, "two halves, each a name and one gloss"),
    "orchestration/who-provides-which-piece": (41, "eight row labels and four column headers"),
    "orchestration/the-stack": (112, "seven bands, each a name and a line, and the two halves named"),
    "orchestration/which-product": (120, "the same seven bands, plus six products and what each answers"),
    "orchestration/ansible-pushes": (70, "thirty real host names, and the controller named"),
    "orchestration/ansible-steps": (72, "four Ansible tasks quoted, and the idempotence rule stated"),
    "orchestration/a-promise": (76, "three hosts named twice, and the two properties named"),
    "orchestration/on-demand": (100, "three moments on a timeline, each with a one-line gloss"),
    "orchestration/where-golem-sits": (126, "the seven bands and the five jobs, plus who answers each"),
    "machine-lifecycle/today": (74, "five steps, each named and glossed, plus who does each of them"),
    "machine-lifecycle/order-install-partition": (60, "three steps, each a name and what is chosen in it"),
    "machine-lifecycle/the-basics": (78, "six tasks of the play, and the idempotence rule stated"),
    "machine-lifecycle/the-services": (78, "four kinds of unit, the legend, and the inventory's counts"),
    "machine-lifecycle/the-proposal": (74, "the same five steps as 01, answered by three tools"),
    "machine-lifecycle/what-pulumi-is": (70, "four parts of the engine named, and what it compares"),
    "machine-lifecycle/one-resource": (58, "three steps and the nine resource fields, quoted verbatim"),
    "machine-lifecycle/where-a-person-stays": (92, "three limits of the resource, each needing a precise sentence"),
    "machine-lifecycle/what-golem-is": (80, "three hosts named twice, the pipeline, and the reversal stated"),
    "machine-lifecycle/what-changes": (84, "two halves of a comparison, each with its own three labels"),
}


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


# A text element's declared width is the box it may wrap inside, not the ink. A
# title given the full content width leaves most of that box empty, so testing the
# declared box for collisions would forbid drawing anything to the right of a short
# title. This narrows the box to the widest line actually set, placed by textAlign.
def inked_extent(element: dict) -> tuple[float, float, float, float]:
    font_family = element.get("fontFamily", 1)
    drawn = min(
        element["width"],
        measured_width(element["text"], element["fontSize"], font_family),
    )
    slack = element["width"] - drawn
    offset = {"left": 0.0, "center": slack / 2.0, "right": slack}[element["textAlign"]]
    left = element["x"] + offset
    return left, element["y"], left + drawn, element["y"] + element["height"]


def overlaps(
    first: tuple[float, float, float, float], second: tuple[float, float, float, float]
) -> bool:
    return (
        first[0] < second[2]
        and second[0] < first[2]
        and first[1] < second[3]
        and second[1] < first[3]
    )


def monospace_line_advance(line: str, font_size: float) -> float:
    return len(line) * TRUE_MONOSPACE_ADVANCE * font_size


def body_word_count(elements: list[dict]) -> int:
    total = 0
    title_skipped = False
    for element in elements:
        if element["type"] != "text":
            continue
        if not title_skipped and element["fontSize"] >= TITLE_SIZE:
            title_skipped = True
            continue
        total += len(element["text"].split())
    return total


def slide_documents(documents: dict[str, dict]) -> dict[str, dict]:
    combined = {
        f"{deck.directory}/{deck.combined_filename}" for deck in DECKS
    }
    return {
        name: payload for name, payload in documents.items() if name not in combined
    }


def collapsed(body: str) -> str:
    return " ".join(body.split())


def drawn_text(documents: dict[str, dict], deck_name: str, slug: str) -> list[str]:
    deck = next(deck for deck in DECKS if deck.name == deck_name)
    slide = next(slide for slide in deck.slides if slide.slug == slug)
    payload = documents[f"{deck.directory}/{slide.filename}"]
    return [
        collapsed(element["text"])
        for element in payload["elements"]
        if element["type"] == "text"
    ]


class GeneratedScenes(unittest.TestCase):
    output: Path
    documents: dict[str, dict]

    @classmethod
    def setUpClass(cls) -> None:
        cls._workspace = tempfile.TemporaryDirectory()
        cls.output = Path(cls._workspace.name) / "first"
        build_all(cls.output)
        cls.documents = {
            str(path.relative_to(cls.output)).replace("\\", "/"): json.loads(
                path.read_text(encoding="utf-8")
            )
            for path in sorted(cls.output.rglob("*.excalidraw"))
        }

    @classmethod
    def tearDownClass(cls) -> None:
        cls._workspace.cleanup()

    def test_every_slide_and_every_deck_is_written(self) -> None:
        expected = {ICON_SHEET_FILENAME}
        for deck in DECKS:
            expected.add(f"{deck.directory}/{deck.combined_filename}")
            for slide in deck.slides:
                expected.add(f"{deck.directory}/{slide.filename}")
        self.assertEqual(set(self.documents), expected)

    def test_documents_have_the_excalidraw_envelope(self) -> None:
        for name, payload in self.documents.items():
            with self.subTest(name):
                self.assertEqual(payload["type"], "excalidraw")
                self.assertEqual(payload["version"], 2)
                self.assertIsInstance(payload["elements"], list)
                self.assertTrue(payload["elements"])
                self.assertIsInstance(payload["files"], dict)
                self.assertIn("viewBackgroundColor", payload["appState"])

    def test_every_embedded_file_is_referenced_and_complete(self) -> None:
        for name, payload in self.documents.items():
            referenced = {
                element["fileId"]
                for element in payload["elements"]
                if element["type"] == "image"
            }
            with self.subTest(name):
                self.assertEqual(set(payload["files"]), referenced)
            for file_id, entry in payload["files"].items():
                with self.subTest(name=name, file=file_id):
                    for key in FILE_KEYS:
                        self.assertIn(key, entry)
                    self.assertEqual(entry["id"], file_id)
                    self.assertTrue(
                        entry["dataURL"].startswith(f"data:{entry['mimeType']};base64,")
                    )

    # An image element carrying a wall-clock timestamp would break the byte-identical
    # rebuild that everything else here is written to protect.
    def test_embedded_files_carry_the_generator_timestamp(self) -> None:
        for name, payload in self.documents.items():
            for file_id, entry in payload["files"].items():
                with self.subTest(name=name, file=file_id):
                    self.assertEqual(entry["created"], UPDATED)
                    self.assertEqual(entry["lastRetrieved"], UPDATED)

    def test_image_elements_carry_every_required_key(self) -> None:
        for name, payload in self.documents.items():
            for element in payload["elements"]:
                if element["type"] != "image":
                    continue
                with self.subTest(name=name, element=element["id"]):
                    for key in IMAGE_KEYS:
                        self.assertIn(key, element)
                    self.assertIn(element["fileId"], payload["files"])

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
                    self.assertEqual(element["width"], max(xs) - min(xs))
                    self.assertEqual(element["height"], max(ys) - min(ys))

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

    def test_no_text_falls_below_the_type_floor(self) -> None:
        for name, payload in self.documents.items():
            for element in payload["elements"]:
                if element["type"] != "text":
                    continue
                with self.subTest(name=name, text=element["text"][:40]):
                    self.assertGreaterEqual(element["fontSize"], MINIMUM_SIZE)

    def test_every_slide_stays_within_its_word_budget(self) -> None:
        for deck in DECKS:
            for slide in deck.slides:
                key = f"{deck.name}/{slide.slug}"
                payload = self.documents[f"{deck.directory}/{slide.filename}"]
                budget, reason = WORD_BUDGET_CEILINGS.get(
                    key, (WORDS_PER_SLIDE, "no exemption")
                )
                with self.subTest(slide=key, reason=reason):
                    self.assertLessEqual(body_word_count(payload["elements"]), budget)

    def test_the_goals_slide_states_every_goal(self) -> None:
        stated = drawn_text(self.documents, GOLEM_DECK_NAME, s16_the_goals.SLUG)
        for goal in goals.GOALS:
            with self.subTest(goal=goal.number):
                self.assertIn(collapsed(goal.statement), stated)

    def test_the_scorecard_marks_every_graded_claim_exactly_once(self) -> None:
        marked = [
            body
            for slug in SCORECARD_SLUGS
            for body in drawn_text(self.documents, GOLEM_DECK_NAME, slug)
        ]
        for claim in goals.GRADED_CLAIMS:
            with self.subTest(claim=claim):
                self.assertEqual(marked.count(collapsed(claim)), 1)

    def test_the_scorecard_rows_and_the_graded_claims_are_the_same_list(self) -> None:
        self.assertEqual(
            [row.claim for row in scorecard.ROWS], list(goals.GRADED_CLAIMS)
        )

    def test_every_scorecard_row_reaches_exactly_one_slide(self) -> None:
        shown = (
            s34_grading_goals_one_to_three.ROWS
            + s35_grading_goals_four_and_five.ROWS
        )
        self.assertEqual(shown, scorecard.ROWS)

    def test_a_chip_stack_taller_than_its_evidence_stays_inside_its_row(self) -> None:
        row = scorecard.ScoreRow(
            goals.UNDOABLE,
            "A claim",
            scorecard.ACHIEVED_STATE,
            "One short line.",
            tuple(
                scorecard.Chip(label, scorecard.QUALIFIED_STATE)
                for label in ("first", "second", "third")
            ),
        )
        scene = Scene("chip-stack-probe")
        scorecard.draw(scene, (row,))

        (card,) = [
            element
            for element in scene.elements
            if element["type"] == "rectangle"
            and element["width"] == scorecard.CARD_WIDTH
        ]
        (claim,) = [
            element
            for element in scene.elements
            if element["type"] == "text"
            and element["fontSize"] == scorecard.CLAIM_SIZE
        ]
        evidence_top = claim["y"] + claim["height"] + scorecard.CLAIM_GAP
        containers = {
            element["containerId"]
            for element in scene.elements
            if element["type"] == "text" and element["fontFamily"] == MONO
        }
        chips = [
            element for element in scene.elements if element["id"] in containers
        ]

        self.assertEqual(len(chips), len(row.chips))
        for chip in chips:
            with self.subTest(chip=chip["id"]):
                self.assertGreaterEqual(chip["y"], evidence_top)
                self.assertLessEqual(
                    chip["y"] + chip["height"],
                    card["y"] + card["height"] - scorecard.ROW_PADDING,
                )

    def test_the_diff_slide_names_every_glyph_op(self) -> None:
        stated = drawn_text(self.documents, GOLEM_DECK_NAME, s39_the_diff.SLUG)
        for name in glyph_ops.OP_NAMES:
            with self.subTest(op=name):
                self.assertIn(name, stated)

    def test_every_enactment_frame_keys_all_four_glyph_ops(self) -> None:
        for slug in ENACTMENT_SLUGS:
            drawn = drawn_text(self.documents, GOLEM_DECK_NAME, slug)
            for op in glyph_ops.OPS:
                with self.subTest(slide=slug, op=op.name):
                    self.assertIn(op.verb, drawn)

    def test_the_enactment_frames_draw_the_same_hosts(self) -> None:
        for slug in ENACTMENT_SLUGS:
            drawn = drawn_text(self.documents, GOLEM_DECK_NAME, slug)
            for host in enactment.SHOWN_HOSTS:
                with self.subTest(slide=slug, host=host):
                    self.assertIn(host, drawn)

    def test_every_enactment_frame_panels_its_hosts_in_one_order(self) -> None:
        for module in ENACTMENT_MODULES:
            with self.subTest(slide=module.SLUG):
                self.assertEqual(
                    tuple(panel.name for panel in module.panels()),
                    enactment.SHOWN_HOSTS,
                )

    def test_each_host_journal_only_grows_across_the_enactment_frames(self) -> None:
        for position, host in enumerate(enactment.SHOWN_HOSTS):
            counts = [module.panels()[position].revisions for module in ENACTMENT_MODULES]
            for index, (earlier, later) in enumerate(zip(counts, counts[1:])):
                with self.subTest(host=host, after=ENACTMENT_SLUGS[index + 1]):
                    self.assertLessEqual(earlier, later)

    def test_every_enactment_frame_draws_the_revisions_its_panels_claim(self) -> None:
        for module in ENACTMENT_MODULES:
            drawn = drawn_text(self.documents, GOLEM_DECK_NAME, module.SLUG)
            for panel in module.panels():
                for row in enactment.revision_rows(panel.revisions):
                    with self.subTest(slide=module.SLUG, revision=row):
                        self.assertIn(collapsed(row), drawn)

    def test_the_record_frame_enacts_the_plan_frame_before_it(self) -> None:
        planned, applied = PLANNED_THEN_APPLIED
        self.assertEqual(
            [(panel.name, panel.rows) for panel in planned.panels()],
            [(panel.name, panel.rows) for panel in applied.panels()],
        )

    def test_a_plan_frame_leaves_every_journal_where_it_found_it(self) -> None:
        applied = s20_after_the_first_apply.panels()
        planned = s21_plan_one_host_changes.panels()
        self.assertEqual(
            [panel.revisions for panel in applied],
            [panel.revisions for panel in planned],
        )

    def test_a_plan_frame_points_at_a_cell_for_every_op_that_changes_one(self) -> None:
        for module in ENACTMENT_MODULES:
            for panel in module.panels():
                for row in panel.rows:
                    with self.subTest(slide=module.SLUG, host=panel.name, op=row.op.name):
                        self.assertEqual(bool(row.slots), row.op.changes_cells)
                        for slot in row.slots:
                            self.assertEqual(panel.cells[slot].op, row.op)

    def test_word_budget_ceilings_name_a_real_slide(self) -> None:
        known = {
            f"{deck.name}/{slide.slug}" for deck in DECKS for slide in deck.slides
        }
        self.assertEqual(set(WORD_BUDGET_CEILINGS) - known, set())

    def test_each_deck_holds_one_frame_per_slide(self) -> None:
        for deck in DECKS:
            payload = self.documents[f"{deck.directory}/{deck.combined_filename}"]
            frames = [
                element
                for element in payload["elements"]
                if element["type"] == "frame"
            ]
            with self.subTest(deck.name):
                self.assertEqual(len(frames), len(deck.slides))
                self.assertEqual(
                    [frame["name"] for frame in frames],
                    [slide.frame_name for slide in deck.slides],
                )
                for element in payload["elements"]:
                    if element["type"] != "frame":
                        self.assertIsNotNone(element["frameId"])

    def test_slide_elements_stay_inside_the_canvas_margin(self) -> None:
        for name, payload in slide_documents(self.documents).items():
            for element in payload["elements"]:
                left, top, right, bottom = extent(element)
                with self.subTest(name=name, element=element["id"]):
                    self.assertGreaterEqual(left, MARGIN_LEFT)
                    self.assertGreaterEqual(top, MARGIN_TOP)
                    self.assertLessEqual(right, MARGIN_RIGHT)
                    self.assertLessEqual(bottom, MARGIN_BOTTOM)

    def test_no_image_sits_on_drawn_text(self) -> None:
        for name, payload in slide_documents(self.documents).items():
            images = [
                element for element in payload["elements"] if element["type"] == "image"
            ]
            if not images:
                continue
            for image in images:
                for element in payload["elements"]:
                    if element["type"] != "text" or element.get("containerId"):
                        continue
                    with self.subTest(
                        name=name, image=image["id"], text=element["text"][:32]
                    ):
                        self.assertFalse(
                            overlaps(extent(image), inked_extent(element))
                        )

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
                room = container["width"] - 2 * BOUND_TEXT_PADDING - MINIMUM_LABEL_SLACK
                for line in element["text"].split("\n"):
                    with self.subTest(name=name, element=element["id"], line=line):
                        self.assertLessEqual(
                            monospace_line_advance(line, element["fontSize"]), room
                        )

    def test_two_independent_builds_are_byte_identical(self) -> None:
        with tempfile.TemporaryDirectory() as workspace:
            repeat = Path(workspace) / "second"
            build_all(repeat)
            first = sorted(
                path.relative_to(self.output) for path in self.output.rglob("*.excalidraw")
            )
            second = sorted(
                path.relative_to(repeat) for path in repeat.rglob("*.excalidraw")
            )
            self.assertEqual(first, second)
            for relative in first:
                with self.subTest(str(relative)):
                    self.assertEqual(
                        (self.output / relative).read_bytes(),
                        (repeat / relative).read_bytes(),
                    )


def segment_meets_box(
    start: tuple[float, float],
    end: tuple[float, float],
    box: tuple[float, float, float, float],
) -> bool:
    left, top, right, bottom = box
    steps = 200
    for step in range(steps + 1):
        along = step / steps
        point_x = start[0] + (end[0] - start[0]) * along
        point_y = start[1] + (end[1] - start[1]) * along
        if left <= point_x <= right and top <= point_y <= bottom:
            return True
    return False


# A transition label sat a fixed distance straight above the bow vertex. Both arrow
# segments radiate from that vertex, so the offset only cleared them while the arrow
# was locally horizontal — on a vertical transition the arrow was drawn through the
# glyphs. These two cases are the ones that failed.
class StateMachineLabels(unittest.TestCase):
    NODE_WIDTH = 240.0
    NODE_HEIGHT = 90.0

    def label_and_arrow(
        self, target_x: float, target_y: float, bow: float
    ) -> tuple[dict, dict]:
        scene = Scene(f"transition-probe-{target_x}-{target_y}-{bow}")
        state_machine(
            scene,
            (
                StateNode(
                    "from", "From", 400.0, 400.0, NEUTRAL, self.NODE_WIDTH, self.NODE_HEIGHT
                ),
                StateNode(
                    "to", "To", target_x, target_y, NEUTRAL, self.NODE_WIDTH, self.NODE_HEIGHT
                ),
            ),
            (Transition("from", "to", "a labelled move", bow=bow),),
        )
        arrow = next(
            element for element in scene.elements if element["type"] == "arrow"
        )
        label = next(
            element
            for element in scene.elements
            if element["type"] == "text" and element["text"] == "a labelled move"
        )
        return arrow, label

    def assert_arrow_clears_label(self, arrow: dict, label: dict) -> None:
        box = (
            label["x"],
            label["y"],
            label["x"] + label["width"],
            label["y"] + label["height"],
        )
        absolute = [
            (arrow["x"] + point[0], arrow["y"] + point[1]) for point in arrow["points"]
        ]
        for start, end in zip(absolute, absolute[1:]):
            self.assertFalse(segment_meets_box(start, end, box))

    def test_a_vertical_transition_does_not_strike_through_its_label(self) -> None:
        for bow in (-70.0, 70.0):
            with self.subTest(bow=bow):
                arrow, label = self.label_and_arrow(400.0, 700.0, bow)
                self.assert_arrow_clears_label(arrow, label)

    def test_a_horizontal_transition_does_not_strike_through_its_label(self) -> None:
        for bow in (-60.0, 60.0):
            with self.subTest(bow=bow):
                arrow, label = self.label_and_arrow(900.0, 400.0, bow)
                self.assert_arrow_clears_label(arrow, label)

    def test_opposing_transitions_put_their_labels_on_opposite_sides(self) -> None:
        forward, forward_label = self.label_and_arrow(900.0, 400.0, 60.0)
        scene = Scene("transition-probe-reverse")
        state_machine(
            scene,
            (
                StateNode(
                    "from", "From", 900.0, 400.0, NEUTRAL, self.NODE_WIDTH, self.NODE_HEIGHT
                ),
                StateNode(
                    "to", "To", 400.0, 400.0, NEUTRAL, self.NODE_WIDTH, self.NODE_HEIGHT
                ),
            ),
            (Transition("from", "to", "a labelled move", bow=60.0),),
        )
        reverse = next(
            element for element in scene.elements if element["type"] == "arrow"
        )
        reverse_label = next(
            element
            for element in scene.elements
            if element["type"] == "text" and element["text"] == "a labelled move"
        )
        self.assert_arrow_clears_label(forward, forward_label)
        self.assert_arrow_clears_label(reverse, reverse_label)


class Icons(unittest.TestCase):
    def test_every_icon_draws_inside_its_declared_bounding_box(self) -> None:
        origin_x, origin_y = ICON_PROBE_ORIGIN
        for spec in icons.CATALOGUE:
            with self.subTest(spec.name):
                scene = Scene(f"icon-probe-{spec.name}")
                mark = spec.draw(scene, origin_x, origin_y, ICON_PROBE_SIZE)
                self.assertTrue(mark.elements)
                self.assertEqual(len(mark.elements), len(scene.elements))
                self.assertAlmostEqual(
                    mark.width, spec.aspect * ICON_PROBE_SIZE, places=6
                )
                self.assertAlmostEqual(mark.height, ICON_PROBE_SIZE, places=6)
                for element in mark.elements:
                    left, top, right, bottom = extent(element)
                    self.assertGreaterEqual(left, mark.x - GEOMETRY_TOLERANCE)
                    self.assertGreaterEqual(top, mark.y - GEOMETRY_TOLERANCE)
                    self.assertLessEqual(right, mark.right + GEOMETRY_TOLERANCE)
                    self.assertLessEqual(bottom, mark.bottom + GEOMETRY_TOLERANCE)

    def test_no_icon_draws_text(self) -> None:
        for spec in icons.CATALOGUE:
            with self.subTest(spec.name):
                scene = Scene(f"icon-text-probe-{spec.name}")
                mark = spec.draw(scene, 0.0, 0.0, ICON_PROBE_SIZE)
                self.assertEqual(
                    [element for element in mark.elements if element["type"] == "text"],
                    [],
                )

    def test_replica_set_never_overlaps_itself_at_any_count(self) -> None:
        for replicas in (1, 2, 3, 4, 6, 8):
            with self.subTest(replicas=replicas):
                scene = Scene(f"replica-probe-{replicas}")
                mark = icons.replica_set(
                    scene, 0.0, 0.0, ICON_PROBE_SIZE, replicas=replicas
                )
                bodies = [
                    element
                    for element in mark.elements
                    if element["type"] == "rectangle"
                ]
                self.assertEqual(len(bodies), replicas)
                for left, right in zip(bodies, bodies[1:]):
                    self.assertLessEqual(
                        left["x"] + left["width"], right["x"] + GEOMETRY_TOLERANCE
                    )
                for element in mark.elements:
                    edges = extent(element)
                    self.assertGreaterEqual(edges[0], mark.x - GEOMETRY_TOLERANCE)
                    self.assertLessEqual(edges[2], mark.right + GEOMETRY_TOLERANCE)

    def test_the_catalogue_names_are_unique(self) -> None:
        names = [spec.name for spec in icons.CATALOGUE]
        self.assertEqual(len(names), len(set(names)))


if __name__ == "__main__":
    unittest.main()
