from __future__ import annotations

from excalidraw.layout import note, slide_header
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene

from . import enactment
from .glyph_ops import INSTALL, NOOP, REMOVE, REPLACE

SLUG = "plan-a-change"
TITLE = "The plan for a change"

SUBTITLE = (
    "Every glyph golemd compares becomes one of four operations. "
    "golem already keeps these three hosts."
)

CLOSING = (
    "golemctl sends the manifest to each host. Each golemd computes its own "
    "plan and returns it."
)

REVISIONS = 2

REMOVED_SLOT = enactment.CELL_COLUMNS - 1


def panels() -> tuple[enactment.HostPanel, ...]:
    cobar = enactment.units_on("cobar")
    dingo = enactment.units_on("dingo")
    achoo = enactment.units_on("achoo")
    return (
        enactment.HostPanel(
            "cobar",
            enactment.present(REMOVED_SLOT, NOOP)
            + enactment.present(1, REMOVE)
            + enactment.present(cobar - REMOVED_SLOT - 1, NOOP),
            (
                enactment.OpRow(REMOVE, 1, (REMOVED_SLOT,)),
                enactment.OpRow(NOOP, cobar - 1),
            ),
            revisions=REVISIONS,
        ),
        enactment.HostPanel(
            "dingo",
            enactment.present(dingo, NOOP) + enactment.absent(1, INSTALL),
            (
                enactment.OpRow(INSTALL, 1, (dingo,)),
                enactment.OpRow(NOOP, dingo),
            ),
            revisions=REVISIONS,
        ),
        enactment.HostPanel(
            "achoo",
            enactment.present(achoo - 1, NOOP) + enactment.present(1, REPLACE),
            (
                enactment.OpRow(REPLACE, 1, (achoo - 1,)),
                enactment.OpRow(NOOP, achoo - 1),
            ),
            revisions=REVISIONS,
        ),
    )


def build() -> Scene:
    scene = Scene(SLUG)
    header_bottom = slide_header(scene, TITLE, SUBTITLE)
    enactment.draw(scene, panels(), enactment.PLAN, header_bottom)
    note(scene, MARGIN, enactment.NOTE_Y, CLOSING, width=CONTENT_WIDTH)
    return scene
