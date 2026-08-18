from __future__ import annotations

from excalidraw.layout import note, slide_header
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene

from . import enactment
from .glyph_ops import INSTALL, NOOP, REMOVE, REPLACE

SLUG = "the-change-applied"
TITLE = "The change applied"

SUBTITLE = (
    "The same three hosts, with the plan enacted and one revision added to "
    "each journal."
)

CLOSING = "Each host wrote its own revision. Nothing recorded it centrally."

REVISION = 3

REMOVED_SLOT = enactment.CELL_COLUMNS - 1


def panels() -> tuple[enactment.HostPanel, ...]:
    cobar = enactment.units_on("cobar")
    dingo = enactment.units_on("dingo")
    achoo = enactment.units_on("achoo")
    return (
        enactment.HostPanel(
            "cobar",
            enactment.present(REMOVED_SLOT, NOOP)
            + enactment.absent(1, REMOVE)
            + enactment.present(cobar - REMOVED_SLOT - 1, NOOP),
            (
                enactment.OpRow(REMOVE, 1, (REMOVED_SLOT,)),
                enactment.OpRow(NOOP, cobar - 1),
            ),
            revisions=REVISION,
        ),
        enactment.HostPanel(
            "dingo",
            enactment.present(dingo, NOOP) + enactment.present(1, INSTALL),
            (
                enactment.OpRow(INSTALL, 1, (dingo,)),
                enactment.OpRow(NOOP, dingo),
            ),
            revisions=REVISION,
        ),
        enactment.HostPanel(
            "achoo",
            enactment.present(achoo - 1, NOOP) + enactment.present(1, REPLACE),
            (
                enactment.OpRow(REPLACE, 1, (achoo - 1,)),
                enactment.OpRow(NOOP, achoo - 1),
            ),
            revisions=REVISION,
        ),
    )


def build() -> Scene:
    scene = Scene(SLUG)
    header_bottom = slide_header(scene, TITLE, SUBTITLE)
    enactment.draw(scene, panels(), enactment.record(REVISION), header_bottom)
    note(scene, MARGIN, enactment.NOTE_Y, CLOSING, width=CONTENT_WIDTH)
    return scene
