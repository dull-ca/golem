"""The three mark states a scorecard row can carry, and the row shape itself.

`ROWS` grades all seven of `goals.GRADED_CLAIMS`; they do not fit one canvas
at the type floor, so `s40_grading_goals_one_to_three` and
`s41_grading_goals_four_and_five` each draw a slice through
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


class Chip(NamedTuple):
    label: str
    state: MarkState | None = None


class ScoreRow(NamedTuple):
    goal_number: int
    claim: str
    state: MarkState
    evidence: str
    chips: tuple[Chip, ...] = ()


(EVERY_STEP_UNDOABLE,) = goals.goal(goals.UNDOABLE).graded_claims
(STATIC_ANALYSIS_POSSIBLE,) = goals.goal(goals.ANALYSABLE).graded_claims
EASIER_TO_PLAN, CERTAIN_A_CHANGE_WORKS = goals.goal(goals.PLANNABLE).graded_claims
ROLLBACK_ON_ONE_HOST, ROLLBACK_ACROSS_THE_FLEET = goals.goal(
    goals.ROLLBACK
).graded_claims
(NO_YAML,) = goals.goal(goals.NO_YAML).graded_claims

ROWS: tuple[ScoreRow, ...] = (
    ScoreRow(
        goals.UNDOABLE,
        EVERY_STEP_UNDOABLE,
        ACHIEVED_STATE,
        "All four glyph kinds reverse. lineInFile leaves an empty file it "
        "created, and a newline it added; only the directory glyph records the "
        "parent directories golem makes. A failed reverse still marks the step "
        "reversed.",
        (
            Chip("lineInFile", QUALIFIED_STATE),
            Chip("parent dirs", QUALIFIED_STATE),
        ),
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
        (Chip("policy = keep"),),
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
CHIP_MARK_SIZE = 28.0
CHIP_MARK_GAP = 10.0
CHIP_STACK_GAP = 10.0


def _chip_mark_span(chip: Chip) -> float:
    return 0.0 if chip.state is None else CHIP_MARK_SIZE + CHIP_MARK_GAP


def _chip_state(row: ScoreRow, chip: Chip) -> MarkState:
    return row.state if chip.state is None else chip.state


def _chip_column_width(row: ScoreRow) -> float:
    return max(
        (
            _chip_mark_span(chip)
            + fit_width(chip.label, CAPTION_SIZE, font_family=MONO)
            for chip in row.chips
        ),
        default=0.0,
    )


def _chip_column_height(row: ScoreRow) -> float:
    if not row.chips:
        return 0.0
    return len(row.chips) * CHIP_HEIGHT + (len(row.chips) - 1) * CHIP_STACK_GAP


def _evidence_width(row: ScoreRow) -> float:
    if not row.chips:
        return TEXT_WIDTH
    return TEXT_WIDTH - _chip_column_width(row) - CHIP_GAP


def _evidence_block_height(row: ScoreRow) -> float:
    _, evidence = _laid_out(row)
    return max(measured_height(evidence, EVIDENCE_SIZE), _chip_column_height(row))


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
    claim, _ = _laid_out(row)
    return (
        2 * ROW_PADDING
        + measured_height(claim, CLAIM_SIZE)
        + CLAIM_GAP
        + _evidence_block_height(row)
    )


def _draw_chips(scene: Scene, row: ScoreRow, evidence_top: float) -> None:
    column_width = _chip_column_width(row)
    column_left = CARD_X + TEXT_X_OFFSET + TEXT_WIDTH - column_width
    top = evidence_top + (_evidence_block_height(row) - _chip_column_height(row)) / 2.0
    for chip in row.chips:
        mark_span = _chip_mark_span(chip)
        if chip.state is not None:
            chip.state.mark(
                scene,
                column_left,
                top + (CHIP_HEIGHT - CHIP_MARK_SIZE) / 2.0,
                CHIP_MARK_SIZE,
            )
        tone = _chip_state(row, chip).tone
        scene.rectangle(
            column_left + mark_span,
            top,
            column_width - mark_span,
            CHIP_HEIGHT,
            Tone(tone.stroke, WHITE, tone.stroke),
            label=chip.label,
            label_font_size=CAPTION_SIZE,
            label_font_family=MONO,
        )
        top += CHIP_HEIGHT + CHIP_STACK_GAP


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
    _draw_chips(scene, row, evidence_top)
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
