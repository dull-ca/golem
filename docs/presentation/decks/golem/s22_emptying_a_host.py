from __future__ import annotations

from excalidraw.layout import connector, note, slide_header
from excalidraw.palette import GAP, INK_SOFT
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import CAPTION_SIZE

from . import enactment
from .glyph_ops import NOOP, REMOVE

SLUG = "emptying-a-host"
TITLE = "Emptying a host: removing what golem put on dingo"

SUBTITLE = (
    "There is no decommission verb. Removing everything is reconciling toward "
    "an empty scroll."
)

BAND_HEADING = "applied"

EMPTIED_HOST = "dingo"
SOURCE_X = 60.0
SOURCE_CAPTION_X = 96.0
SOURCE_CAPTION = "in the journal, not in the scroll"

CLOSING = (
    "Undo is not exact: a file golem created for one line is left empty, and "
    "directories it made along the way stay. Anything golem did not put there "
    "was never touched."
)


def _kept(name: str) -> enactment.HostPanel:
    return enactment.HostPanel(
        name,
        enactment.all_cells(name, NOOP, present=True),
        (enactment.PlanRow(NOOP, enactment.units_on(name)),),
        revisions=3,
    )


def _emptied() -> enactment.HostPanel:
    return enactment.HostPanel(
        EMPTIED_HOST,
        enactment.all_cells(EMPTIED_HOST, REMOVE, present=False),
        (
            enactment.PlanRow(
                REMOVE,
                enactment.units_on(EMPTIED_HOST),
                enactment.every_slot(EMPTIED_HOST),
            ),
        ),
        revisions=3,
    )


def panels() -> tuple[enactment.HostPanel, ...]:
    return tuple(
        _emptied() if name == EMPTIED_HOST else _kept(name)
        for name in enactment.SHOWN_HOSTS
    )


def _journal_is_the_source(scene: Scene) -> None:
    x = enactment.column_x(enactment.SHOWN_HOSTS.index(EMPTIED_HOST))
    connector(
        scene,
        [
            (x + SOURCE_X, enactment.JOURNAL_Y - 6),
            (x + SOURCE_X, enactment.MACHINE_BOTTOM + 6),
        ],
        stroke=REMOVE.tone.stroke,
        dashed=True,
    )
    scene.text(
        x + SOURCE_CAPTION_X,
        enactment.MACHINE_BOTTOM + 12,
        SOURCE_CAPTION,
        font_size=CAPTION_SIZE,
        colour=INK_SOFT,
        width=enactment.COLUMN_WIDTH - SOURCE_CAPTION_X,
    )


def build() -> Scene:
    scene = Scene(SLUG)
    header_bottom = slide_header(scene, TITLE, SUBTITLE)
    enactment.draw(scene, panels(), BAND_HEADING, header_bottom)
    _journal_is_the_source(scene)
    note(
        scene,
        MARGIN,
        enactment.NOTE_Y,
        CLOSING,
        width=CONTENT_WIDTH,
        colour=GAP.stroke,
    )
    return scene
