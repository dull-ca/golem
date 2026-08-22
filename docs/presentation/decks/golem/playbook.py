"""Four playbook steps, and the fleet state each one leaves behind.

s11-s14 each ask for the state after N steps and draw the same play beside
it, so the task list and the fleet page forward together.

The three step bodies are Ansible's own task shapes -- a file, a line in a
file, a container unit -- which are also three of golem's four glyphs
(s48_the_four_glyphs). No slide says so: the glyphs aren't named formally
until s48, so leaving the correspondence uncaptioned here lets that slide
land as recognition instead of new information.

Each step's hosts are a real Ansible pattern: a named group, not the whole
fleet and not one machine. That is the frame's argument, so the group sizes
and the particular hosts change step to step instead of repeating.
"""

from __future__ import annotations

from collections import Counter
from typing import Mapping, NamedTuple

from excalidraw.layout import note
from excalidraw.palette import ANSIBLE
from excalidraw.scene import MARGIN, Scene
from excalidraw.text import HAND
from excalidraw.type_scale import BODY_SIZE

from ..ansible_play import draw_play, play_height, steps_reached
from ..lichess_fleet import HOSTS
from ..machines import FLEET_X, LEGEND_Y, Machine, draw_fleet, swatch_entry
from . import fleet

PLAYBOOK_FILENAME = "site.yml"

PLAY_X = fleet.TOOL_X
PLAY_Y = fleet.TOOL_Y
PLAY_WIDTH = fleet.TOOL_WIDTH
STEP_HEIGHT = 80.0

CELL_CAPTION = "a file, line or workload this play put here"

BARE_FLEET_X = MARGIN
BARE_FLEET_Y = 196.0
BARE_FLEET_WIDTH = 336.0
BARE_FLEET_NOTE = (
    "The fleet is drawn empty here, so the only cells on it are the ones "
    "this play adds."
)


class PlayStep(NamedTuple):
    body: str
    hosts: tuple[str, ...]


STEPS: tuple[PlayStep, ...] = (
    PlayStep("add file", ("achoo", "cobar", "dingo")),
    PlayStep("add line to file", ("manta", "orbit")),
    PlayStep("add file", ("radio", "snafu", "zulip")),
    PlayStep("add workload", ("cobar", "orbit", "talos")),
)

CHANGES_MADE = sum(len(step.hosts) for step in STEPS)
HOSTS_CHANGED = len({host for step in STEPS for host in step.hosts})


def _cells_placed_by(steps_taken: int) -> Mapping[str, int]:
    tally: Counter[str] = Counter()
    for step in STEPS[:steps_taken]:
        tally.update(step.hosts)
    return tally


def machines_after(steps_taken: int) -> tuple[Machine, ...]:
    placed = _cells_placed_by(steps_taken)
    # A unit cell is only about 35x23px at this geometry
    # (machines.cell_rect), with no room for a per-step icon, so the step
    # kind is carried by the play row's text and every cell here gets the
    # one tone regardless of which step placed it.
    return tuple(
        Machine(
            host.name,
            tool_units=placed[host.name],
            keeper=ANSIBLE,
            unit_tone=ANSIBLE,
        )
        for host in HOSTS
    )


def draw_frame(scene: Scene, steps_taken: int) -> None:
    draw_fleet(scene, machines_after(steps_taken))
    swatch_entry(scene, FLEET_X, LEGEND_Y, ANSIBLE, "solid", CELL_CAPTION)
    note(
        scene,
        BARE_FLEET_X,
        BARE_FLEET_Y,
        BARE_FLEET_NOTE,
        width=BARE_FLEET_WIDTH,
        font_size=BODY_SIZE,
    )
    draw_play(
        scene,
        PLAY_X,
        PLAY_Y,
        PLAY_WIDTH,
        tuple(step.body for step in STEPS),
        filename=PLAYBOOK_FILENAME,
        step_font_family=HAND,
        step_height=STEP_HEIGHT,
        step_states=steps_reached(steps_taken, len(STEPS)),
    )
    fleet.reaches_the_fleet(
        scene,
        PLAY_Y + play_height(len(STEPS), STEP_HEIGHT) / 2.0,
        ANSIBLE.stroke,
    )
