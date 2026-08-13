"""Ten slides on the five steps that bring a lichess machine into service.

An argument, not a primer. 01 draws today: four of the five steps are done by
hand and Ansible covers the one in the middle. 05 draws the proposal on the same
figure. 02 to 04 are the three spans of today taken one at a time, and 06 to 10
are the two tools that would take over — what they are, what one Pulumi resource
actually covers, and what it leaves with a person.

Ansible's explanation sits on 03 rather than in the block with Pulumi's and
golem's, because on this deck Ansible is not a proposal: it already does step 4
and the proposal leaves it there.
"""

from __future__ import annotations

NAME = "machine-lifecycle"
TITLE = "How a machine comes to exist"

SLIDE_MODULE_NAMES: tuple[str, ...] = (
    "s01_today",
    "s02_order_install_partition",
    "s03_the_basics",
    "s04_the_services",
    "s05_the_proposal",
    "s06_what_pulumi_is",
    "s07_one_resource",
    "s08_where_a_person_stays",
    "s09_what_golem_is",
    "s10_what_changes",
)
