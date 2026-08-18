"""The five goals the deck's author had for golem before he built it.

`s16_the_goals` draws `Goal.statement` verbatim, his own line. The grading
slides (`s36_grading_goals_one_to_three`, `s37_grading_goals_four_and_five`)
grade `Goal.graded_claims` instead: goal 3 packs two claims into one
sentence ("easier to plan" and "being certain a change will work"), graded
separately, so a goal and its claim count are not always the same.
`test_scenes.py` checks the goals slide states every `GOALS` entry and the
scorecard marks every `GRADED_CLAIMS` entry exactly once -- the same defence
`vocabulary.py` uses to keep its five orchestration jobs from drifting
between decks, applied here to keep one deck's own slides from drifting
apart.

This lives in `decks/golem/`, not `decks/vocabulary.py`: that module is
scoped to strings both decks must agree on, and only the golem deck reads
the goals.
"""

from __future__ import annotations

from typing import NamedTuple


class Goal(NamedTuple):
    number: int
    statement: str
    claims_graded_separately: tuple[str, ...] = ()

    @property
    def graded_claims(self) -> tuple[str, ...]:
        return self.claims_graded_separately or (self.statement.rstrip("."),)


GOALS: tuple[Goal, ...] = (
    Goal(1, "Every step undoable."),
    Goal(2, "Static analysis and verification possible."),
    Goal(
        3,
        "Easier to plan / be certain things will work.",
        ("Easier to plan", "Being certain a change will work"),
    ),
    Goal(
        4,
        "Automated rollback on failure.",
        ("Automated rollback on one host", "Automated rollback across the fleet"),
    ),
    Goal(5, "No YAML."),
)

UNDOABLE, ANALYSABLE, PLANNABLE, ROLLBACK, NO_YAML = (goal.number for goal in GOALS)

GRADED_CLAIMS: tuple[str, ...] = tuple(
    claim for goal in GOALS for claim in goal.graded_claims
)


def goal(number: int) -> Goal:
    return GOALS[number - 1]
