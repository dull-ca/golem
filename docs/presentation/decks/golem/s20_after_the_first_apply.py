from __future__ import annotations

from excalidraw.layout import note, slide_header
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene

from . import enactment
from .glyph_ops import INSTALL

SLUG = "after-the-first-apply"
TITLE = "After the first apply: on the hosts, and in the journal"

SUBTITLE = "Each host now carries what its scroll named, and has written down what it did."

BAND_HEADING = "applied"

CLOSING = (
    "Each host keeps one append-only journal: every operation golem applied, "
    "and the inverse that undoes it."
)


def panels() -> tuple[enactment.HostPanel, ...]:
    return tuple(
        enactment.HostPanel(
            name,
            enactment.all_cells(name, INSTALL, present=True),
            (
                enactment.PlanRow(
                    INSTALL, enactment.units_on(name), enactment.every_slot(name)
                ),
            ),
            revisions=2,
        )
        for name in enactment.SHOWN_HOSTS
    )


def build() -> Scene:
    scene = Scene(SLUG)
    header_bottom = slide_header(scene, TITLE, SUBTITLE)
    enactment.draw(scene, panels(), BAND_HEADING, header_bottom)
    note(scene, MARGIN, enactment.NOTE_Y, CLOSING, width=CONTENT_WIDTH)
    return scene
