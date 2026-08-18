from __future__ import annotations

from excalidraw.layout import note, slide_header
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene

from . import enactment
from .glyph_ops import INSTALL, NOOP, REPLACE

SLUG = "plan-one-host-changes"
TITLE = "A plan that changes one host and leaves two alone"

SUBTITLE = (
    "Two hosts answered Noop for everything they carry, so nothing on them changes."
)

BAND_HEADING = "plan"

CHANGED_HOST = "achoo"
REPLACED_SLOT = 1
ADDED_SLOT = 2

CLOSING = (
    "A plan writes no revision, and by default reads nothing on the host: it "
    "diffs a scroll against a journal."
)


def _unchanged(name: str) -> enactment.HostPanel:
    return enactment.HostPanel(
        name,
        enactment.all_cells(name, NOOP, present=True),
        (enactment.PlanRow(NOOP, enactment.units_on(name)),),
        revisions=2,
    )


def _changed() -> enactment.HostPanel:
    kept = enactment.units_on(CHANGED_HOST) - 1
    cells = tuple(
        enactment.Cell(True, REPLACE if slot == REPLACED_SLOT else NOOP)
        for slot in range(enactment.units_on(CHANGED_HOST))
    ) + (enactment.Cell(False, INSTALL),)
    return enactment.HostPanel(
        CHANGED_HOST,
        cells,
        (
            enactment.PlanRow(INSTALL, 1, (ADDED_SLOT,)),
            enactment.PlanRow(REPLACE, 1, (REPLACED_SLOT,)),
            enactment.PlanRow(NOOP, kept),
        ),
        revisions=2,
    )


def panels() -> tuple[enactment.HostPanel, ...]:
    return tuple(
        _changed() if name == CHANGED_HOST else _unchanged(name)
        for name in enactment.SHOWN_HOSTS
    )


def build() -> Scene:
    scene = Scene(SLUG)
    header_bottom = slide_header(scene, TITLE, SUBTITLE)
    enactment.draw(scene, panels(), BAND_HEADING, header_bottom)
    note(scene, MARGIN, enactment.NOTE_Y, CLOSING, width=CONTENT_WIDTH)
    return scene
