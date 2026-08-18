"""The three mark states a scorecard row can carry, and the row shape itself.

`ROWS` grades all seven of `goals.GRADED_CLAIMS`; they do not fit one canvas
at the type floor, so `s34_grading_goals_one_to_three` and
`s35_grading_goals_four_and_five` each draw a slice through
`rows_for_goals`. The cut sits on a goal boundary rather than splitting the
seven rows down the middle, so a goal that states two claims -- goal 3, goal
4 -- keeps both on the same slide.
"""

from __future__ import annotations

from typing import NamedTuple

from excalidraw import icons
from excalidraw.palette import (
    ACHIEVED,
    INK,
    INK_SOFT,
    NOT_ACHIEVED,
    QUALIFIED,
    Tone,
    WHITE,
)
from excalidraw.scene import CONTENT_WIDTH, LABEL_HEADROOM, MARGIN, Scene, fit_width
from excalidraw.text import LINE_HEIGHT, MONO, measured_height, wrapped
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE, HEADING_SIZE

from . import goals


class MarkState(NamedTuple):
    word: str
    tone: Tone
    mark: icons.IconDrawer


ACHIEVED_STATE = MarkState("achieved", ACHIEVED, icons.achieved)
QUALIFIED_STATE = MarkState("qualified", QUALIFIED, icons.qualified)
NOT_ACHIEVED_STATE = MarkState("not achieved", NOT_ACHIEVED, icons.not_achieved)


class ScoreRow(NamedTuple):
    goal_number: int
    claim: str
    state: MarkState
    evidence: str
    token: str = ""


(EVERY_STEP_UNDOABLE,) = goals.goal(goals.UNDOABLE).graded_claims
(STATIC_ANALYSIS_POSSIBLE,) = goals.goal(goals.ANALYSABLE).graded_claims
EASIER_TO_PLAN, CERTAIN_A_CHANGE_WORKS = goals.goal(goals.PLANNABLE).graded_claims
ROLLBACK_ON_ONE_HOST, ROLLBACK_ACROSS_THE_FLEET = goals.goal(
    goals.ROLLBACK
).graded_claims
(NO_YAML,) = goals.goal(goals.NO_YAML).graded_claims

ROWS: tuple[ScoreRow, ...] = (
    # NOTE: qualified, not achieved as the brief first assumed. lineInFile
    # does not round-trip (an empty file survives a reverse where none
    # existed before, and an added trailing newline is never removed), the
    # parent directories golem creates on apply are not recorded in the
    # inverse, and a failed reverse is logged and left rather than retried
    # (apps/golemd/src/reconcilers.rs, apps/golemd/src/foreman.rs:1747-1749).
    ScoreRow(
        goals.UNDOABLE,
        EVERY_STEP_UNDOABLE,
        QUALIFIED_STATE,
        "All four glyph kinds reverse. lineInFile does not round-trip, and a "
        "failed reverse is logged rather than retried.",
        "lineInFile",
    ),
    # NOTE: the asterisk is on fleet-level analysis, not the type checker.
    # `analyze` implements one rule (per-leaf duplicate glyph key) and misses
    # the repo's own smoke fixture divergence; the checker itself is a real
    # Hindley-Milner front end with compile-time case exhaustiveness
    # (apps/emet/src/infer.rs).
    ScoreRow(
        goals.ANALYSABLE,
        STATIC_ANALYSIS_POSSIBLE,
        QUALIFIED_STATE,
        "The type checker infers types and checks case exhaustiveness. Analysis "
        "across a fleet is one rule, which misses the conflict in the repo's own "
        "smoke fixture.",
    ),
    ScoreRow(
        goals.PLANNABLE,
        EASIER_TO_PLAN,
        ACHIEVED_STATE,
        "Planning against a live host returns a verdict for every glyph.",
    ),
    ScoreRow(
        goals.PLANNABLE,
        CERTAIN_A_CHANGE_WORKS,
        QUALIFIED_STATE,
        "The plan above is a real check against a live host. The compiler "
        "checks nothing against a host, and detects neither cross-glyph "
        "conflicts nor dependency order.",
    ),
    # NOTE: achieved, overturning an earlier read of this as
    # operator-triggered. Rollback fires automatically on retry exhaustion
    # and is the default, OnExhaustConfig::Rollback (apps/golemd/src/config.rs:100);
    # golemctl has no rollback/revert/undo verb at all. Scope is the failing
    # leaf unit, which is the designed failure-isolation boundary, not a
    # shortfall.
    ScoreRow(
        goals.ROLLBACK,
        ROLLBACK_ON_ONE_HOST,
        ACHIEVED_STATE,
        "Rollback is the default when retries are exhausted. It reverses the "
        "failing leaf unit, the failure-isolation boundary. A scroll can opt "
        "out; the one serving golem's documentation does.",
        "policy = keep",
    ),
    ScoreRow(
        goals.ROLLBACK,
        ROLLBACK_ACROSS_THE_FLEET,
        NOT_ACHIEVED_STATE,
        "A fleet apply spawns one task per target, with no barrier between "
        "hosts and no fleet-wide reverse.",
    ),
    ScoreRow(
        goals.NO_YAML,
        NO_YAML,
        ACHIEVED_STATE,
        "The authoring language is Emet; the fleet inventory is TOML.",
    ),
)


def rows_for_goals(first: int, last: int) -> tuple[ScoreRow, ...]:
    return tuple(row for row in ROWS if first <= row.goal_number <= last)


CARD_X = MARGIN
CARD_WIDTH = CONTENT_WIDTH
ROWS_TOP = 176.0
ROW_GAP = 18.0
ROW_PADDING = 20.0

NUMBER_X_OFFSET = 24.0
NUMBER_WIDTH = 40.0
MARK_X_OFFSET = 70.0
MARK_SIZE = 56.0
WORD_X_OFFSET = 146.0
WORD_WIDTH = 200.0
TEXT_X_OFFSET = 366.0
TEXT_WIDTH = CARD_WIDTH - TEXT_X_OFFSET - NUMBER_X_OFFSET

CLAIM_SIZE = HEADING_SIZE
EVIDENCE_SIZE = BODY_SIZE
CLAIM_GAP = 8.0

WRAP_WIDTH = TEXT_WIDTH * LABEL_HEADROOM

CHIP_GAP = 18.0
CHIP_HEIGHT = CAPTION_SIZE * LINE_HEIGHT + 14


def _evidence_width(row: ScoreRow) -> float:
    if not row.token:
        return TEXT_WIDTH
    return TEXT_WIDTH - fit_width(row.token, CAPTION_SIZE, font_family=MONO) - CHIP_GAP


def _laid_out(row: ScoreRow) -> tuple[str, str]:
    return (
        wrapped(row.claim, WRAP_WIDTH, CLAIM_SIZE),
        wrapped(row.evidence, _evidence_width(row) * LABEL_HEADROOM, EVIDENCE_SIZE),
    )


def _row_labels(rows: tuple[ScoreRow, ...]) -> tuple[str, ...]:
    counts: dict[int, int] = {}
    for row in rows:
        counts[row.goal_number] = counts.get(row.goal_number, 0) + 1
    seen: dict[int, int] = {}
    labels: list[str] = []
    for row in rows:
        if counts[row.goal_number] == 1:
            labels.append(f"{row.goal_number}.")
            continue
        seen[row.goal_number] = seen.get(row.goal_number, 0) + 1
        suffix = chr(ord("a") + seen[row.goal_number] - 1)
        labels.append(f"{row.goal_number}{suffix}.")
    return tuple(labels)


def _row_height(row: ScoreRow) -> float:
    claim, evidence = _laid_out(row)
    return (
        2 * ROW_PADDING
        + measured_height(claim, CLAIM_SIZE)
        + CLAIM_GAP
        + measured_height(evidence, EVIDENCE_SIZE)
    )


def _draw_row(scene: Scene, top: float, row: ScoreRow, label: str) -> None:
    height = _row_height(row)
    claim, evidence = _laid_out(row)
    scene.rectangle(CARD_X, top, CARD_WIDTH, height, row.state.tone)
    middle = top + height / 2.0
    scene.text(
        CARD_X + NUMBER_X_OFFSET,
        middle - EVIDENCE_SIZE * LINE_HEIGHT / 2.0,
        label,
        font_size=EVIDENCE_SIZE,
        colour=INK_SOFT,
        width=NUMBER_WIDTH,
    )
    row.state.mark(
        scene, CARD_X + MARK_X_OFFSET, middle - MARK_SIZE / 2.0, MARK_SIZE
    )
    scene.text(
        CARD_X + WORD_X_OFFSET,
        middle - EVIDENCE_SIZE * LINE_HEIGHT / 2.0,
        row.state.word,
        font_size=EVIDENCE_SIZE,
        colour=row.state.tone.text,
        width=WORD_WIDTH,
    )
    scene.text(
        CARD_X + TEXT_X_OFFSET,
        top + ROW_PADDING,
        claim,
        font_size=CLAIM_SIZE,
        colour=INK,
        width=TEXT_WIDTH,
    )
    evidence_top = top + ROW_PADDING + measured_height(claim, CLAIM_SIZE) + CLAIM_GAP
    evidence_width = _evidence_width(row)
    if row.token:
        evidence_height = measured_height(evidence, EVIDENCE_SIZE)
        scene.rectangle(
            CARD_X + TEXT_X_OFFSET + evidence_width + CHIP_GAP,
            evidence_top + (evidence_height - CHIP_HEIGHT) / 2.0,
            TEXT_WIDTH - evidence_width - CHIP_GAP,
            CHIP_HEIGHT,
            Tone(row.state.tone.stroke, WHITE, row.state.tone.stroke),
            label=row.token,
            label_font_size=CAPTION_SIZE,
            label_font_family=MONO,
        )
    scene.text(
        CARD_X + TEXT_X_OFFSET,
        evidence_top,
        evidence,
        font_size=EVIDENCE_SIZE,
        colour=INK_SOFT,
        width=evidence_width,
    )


def draw(scene: Scene, rows: tuple[ScoreRow, ...]) -> None:
    cursor = ROWS_TOP
    for row, label in zip(rows, _row_labels(rows)):
        _draw_row(scene, cursor, row, label)
        cursor += _row_height(row) + ROW_GAP
