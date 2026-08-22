"""The five jobs orchestration is really made of, named once for both decks.

The golem deck expands layer 6 into these five; the orchestration deck takes four
of them a slide at a time. They are the same five things, so they are one tuple:
a rename here moves both decks together, and neither deck can drift into calling
`Health and reconciliation` something else.
"""

from __future__ import annotations

from typing import NamedTuple


class OrchestrationPart(NamedTuple):
    number: int
    title: str
    detail: str


ORCHESTRATION_PARTS: tuple[OrchestrationPart, ...] = (
    OrchestrationPart(1, "Placement", "choosing which node runs a workload"),
    OrchestrationPart(
        2, "Lifecycle", "start, stop, restart, drain, rolling update, rollback"
    ),
    OrchestrationPart(
        3,
        "Health and reconciliation",
        "watch actual state, detect drift or failure, reschedule",
    ),
    OrchestrationPart(
        4,
        "Supporting plumbing",
        "networking, service discovery, load balancers, storage, secrets",
    ),
    OrchestrationPart(5, "Scaling", "replica counts moved by policy or load"),
)

PLACEMENT, LIFECYCLE, HEALTH, PLUMBING, SCALING = (
    part.number for part in ORCHESTRATION_PARTS
)


def part(number: int) -> OrchestrationPart:
    return ORCHESTRATION_PARTS[number - 1]
