from __future__ import annotations

from excalidraw.layout import note, slide_header
from excalidraw.palette import ANSIBLE, INK_FAINT, INK_SOFT, Tone
from excalidraw.scene import MARGIN, Scene, bottom_edge
from excalidraw.type_scale import BODY_SIZE

from ..lichess_fleet import HOST_COUNT, HOSTS
from . import fleet, playbook

SLUG = "what-do-we-undo"
TITLE = "What do we undo?"
SUBTITLE = (
    "Each mark is one thing a machine already carries: a file, a package, "
    "a line or a workload."
)

PRIOR_STATE = Tone(INK_SOFT, INK_FAINT)

MARK_COLUMNS = 10
MARK_ROWS = 5
MARK_GAP = 3.0
MARK_SLOTS = MARK_COLUMNS * MARK_ROWS
FEWEST_MARKS = 36
LATTICE_INSET = 6.0

NOTE_X = MARGIN
NOTE_Y = 372.0
NOTE_WIDTH = 382.0
NOTE_GAP = 26.0

COUNTED = (
    f"The play added {playbook.CHANGES_MADE} of these marks, "
    f"on {playbook.HOSTS_CHANGED} of the {HOST_COUNT} machines."
)
UNRECORDED = (
    "A playbook records what the next run will ask for, not what an earlier "
    "run put on a host. After an edit, the host no longer matches the "
    "playbook that ran."
)


def _machines_under_prior_state() -> tuple[fleet.Machine, ...]:
    return tuple(fleet.Machine(host.name, keeper=ANSIBLE) for host in HOSTS)


def _marks_drawn_on(host_name: str) -> int:
    return FEWEST_MARKS + sum(host_name.encode()) % (MARK_SLOTS - FEWEST_MARKS + 1)


def _lattice_area(
    area: tuple[float, float, float, float]
) -> tuple[float, float, float, float]:
    left, top, width, height = area
    return (
        left + LATTICE_INSET,
        top + LATTICE_INSET,
        width - 2 * LATTICE_INSET,
        height - 2 * LATTICE_INSET,
    )


def _mark_rect(
    area: tuple[float, float, float, float], slot: int
) -> tuple[float, float, float, float]:
    left, top, width, height = area
    mark_width = (width - (MARK_COLUMNS - 1) * MARK_GAP) / MARK_COLUMNS
    mark_height = (height - (MARK_ROWS - 1) * MARK_GAP) / MARK_ROWS
    return (
        left + (slot % MARK_COLUMNS) * (mark_width + MARK_GAP),
        top + (slot // MARK_COLUMNS) * (mark_height + MARK_GAP),
        mark_width,
        mark_height,
    )


def _prior_state_marks(scene: Scene) -> None:
    # NOTE: one loop, one tone, one stroke, one size, and a count taken from
    # the host name alone -- this module never reads playbook.STEPS, only the
    # scalars CHANGES_MADE and HOSTS_CHANGED, which cannot carry a position.
    # A later edit that "helpfully" marked the ones the play added would make
    # this slide argue the opposite of what it says.
    for index, host in enumerate(HOSTS):
        x, y = fleet.machine_origin(index)
        area = _lattice_area(
            fleet.cell_area(x, y, fleet.MACHINE_WIDTH, fleet.MACHINE_HEIGHT)
        )
        for slot in range(_marks_drawn_on(host.name)):
            left, top, mark_width, mark_height = _mark_rect(area, slot)
            scene.rectangle(
                left,
                top,
                mark_width,
                mark_height,
                PRIOR_STATE,
                radius=False,
                stroke_width=1,
            )


def build() -> Scene:
    scene = Scene(SLUG, id_namespace=fleet.ID_NAMESPACE)
    slide_header(scene, TITLE, SUBTITLE)
    fleet.draw_fleet(scene, _machines_under_prior_state())
    _prior_state_marks(scene)
    counted = note(scene, NOTE_X, NOTE_Y, COUNTED, width=NOTE_WIDTH, font_size=BODY_SIZE)
    note(
        scene,
        NOTE_X,
        bottom_edge(counted) + NOTE_GAP,
        UNRECORDED,
        width=NOTE_WIDTH,
        font_size=BODY_SIZE,
    )
    return scene
