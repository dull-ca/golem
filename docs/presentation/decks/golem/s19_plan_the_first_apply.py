from __future__ import annotations

from excalidraw.layout import note, slide_header
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene

from . import enactment
from .glyph_ops import INSTALL

SLUG = "plan-the-first-apply"
TITLE = "The plan before the first apply"

SUBTITLE = (
    "Every difference golemd finds is one of four operations. "
    "Before the first apply they are all Install."
)

BAND_HEADING = "plan"

CLOSING = (
    "golemctl sends the manifest to each host. Each golemd works out its own "
    "plan and answers with it."
)


def panels() -> tuple[enactment.HostPanel, ...]:
    return tuple(
        enactment.HostPanel(
            name,
            enactment.all_cells(name, INSTALL, present=False),
            (
                enactment.PlanRow(
                    INSTALL, enactment.units_on(name), enactment.every_slot(name)
                ),
            ),
            revisions=1,
        )
        for name in enactment.SHOWN_HOSTS
    )


def build() -> Scene:
    scene = Scene(SLUG)
    header_bottom = slide_header(scene, TITLE, SUBTITLE)
    enactment.draw(scene, panels(), BAND_HEADING, header_bottom)
    note(scene, MARGIN, enactment.NOTE_Y, CLOSING, width=CONTENT_WIDTH)
    return scene
