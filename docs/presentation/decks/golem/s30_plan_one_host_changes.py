from __future__ import annotations

from excalidraw.layout import note, slide_header
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene

from . import enactment
from .glyph_ops import INSTALL, NOOP

SLUG = "plan-one-host-changes"
TITLE = "A plan that changes one host and leaves two alone"

SUBTITLE = (
    "Two hosts have Noop for everything they carry: this plan would change "
    "nothing on them."
)

CLOSING = (
    "A plan writes no revision, and by default reads nothing on the host: it "
    "diffs a scroll against a journal. This one was not applied."
)

CHANGED_HOST = "achoo"
REVISIONS = 3


def _unchanged(name: str, units: int) -> enactment.HostPanel:
    return enactment.HostPanel(
        name,
        enactment.present(units, NOOP),
        (enactment.OpRow(NOOP, units),),
        revisions=REVISIONS,
    )


def _changed(units: int) -> enactment.HostPanel:
    return enactment.HostPanel(
        CHANGED_HOST,
        enactment.present(units, NOOP) + enactment.absent(1, INSTALL),
        (
            enactment.OpRow(INSTALL, 1, (units,)),
            enactment.OpRow(NOOP, units),
        ),
        revisions=REVISIONS,
    )


def panels() -> tuple[enactment.HostPanel, ...]:
    return (
        _unchanged("cobar", enactment.units_on("cobar") - 1),
        _unchanged("dingo", enactment.units_on("dingo") + 1),
        _changed(enactment.units_on("achoo")),
    )


def build() -> Scene:
    scene = Scene(SLUG)
    header_bottom = slide_header(scene, TITLE, SUBTITLE)
    enactment.draw(scene, panels(), enactment.PLAN, header_bottom)
    note(scene, MARGIN, enactment.NOTE_Y, CLOSING, width=CONTENT_WIDTH)
    return scene
