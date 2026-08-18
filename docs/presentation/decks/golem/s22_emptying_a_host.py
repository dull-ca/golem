from __future__ import annotations

from excalidraw.layout import connector, note, slide_header
from excalidraw.palette import GAP, INK_FAINT
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.text import MONO, measured_width
from excalidraw.type_scale import CAPTION_SIZE

from . import enactment
from .glyph_ops import NOOP, REMOVE

SLUG = "emptying-a-host"
TITLE = "Emptying a host: removing what golem put on dingo"

SUBTITLE = (
    "There is no decommission verb. Removing everything is reconciling toward "
    "an empty scroll."
)

CLOSING = (
    "Undo is not exact: a file golem created for one line is left empty, and "
    "directories it made along the way stay. Anything golem did not put there "
    "was never touched."
)

EMPTIED_HOST = "dingo"
REVISION = 4
SOURCE_REVISION = 3
SOURCE_CAPTION = "in the journal, not in the scroll"
SOURCE_LEAD = 10.0
SOURCE_CLEARANCE = 6.0
CAPTION_GAP = 20.0
CAPTION_TOP = 8.0


def _kept(name: str, units: int) -> enactment.HostPanel:
    return enactment.HostPanel(
        name,
        enactment.present(units, NOOP),
        (enactment.OpRow(NOOP, units),),
        revisions=REVISION,
    )


def _emptied(units: int) -> enactment.HostPanel:
    return enactment.HostPanel(
        EMPTIED_HOST,
        enactment.absent(units, REMOVE),
        (enactment.OpRow(REMOVE, units, enactment.slots_from(0, units)),),
        revisions=REVISION,
    )


def panels() -> tuple[enactment.HostPanel, ...]:
    return (
        _kept("cobar", enactment.units_on("cobar") - 1),
        _emptied(enactment.units_on("dingo") + 1),
        _kept("achoo", enactment.units_on("achoo")),
    )


def _newest_unit_slot(figure: enactment.Figure) -> int:
    panel = next(p for p in figure.panels if p.name == EMPTIED_HOST)
    return len(panel.cells) - 1


def _journal_is_the_source(scene: Scene, figure: enactment.Figure) -> None:
    x = figure.column_x_of(EMPTIED_HOST)
    row_y = figure.journal_row_y(SOURCE_REVISION)
    row_end = (
        x
        + enactment.JOURNAL_PADDING
        + measured_width(
            enactment.revision_rows(SOURCE_REVISION)[-1], CAPTION_SIZE, MONO
        )
    )
    left, right, _, bottom = figure.slot_span(x, (_newest_unit_slot(figure),))
    lane = (left + right) / 2.0
    connector(
        scene,
        [
            (row_end + SOURCE_LEAD, row_y),
            (lane, row_y),
            (lane, bottom + SOURCE_CLEARANCE),
        ],
        stroke=INK_FAINT,
        dashed=True,
    )
    caption_width = measured_width(SOURCE_CAPTION, CAPTION_SIZE) + 8
    scene.text(
        lane - CAPTION_GAP - caption_width,
        figure.machine_bottom + CAPTION_TOP,
        SOURCE_CAPTION,
        font_size=CAPTION_SIZE,
        colour=INK_FAINT,
        align="right",
        width=caption_width,
    )


def build() -> Scene:
    scene = Scene(SLUG)
    header_bottom = slide_header(scene, TITLE, SUBTITLE)
    figure = enactment.draw(
        scene, panels(), enactment.record(REVISION), header_bottom
    )
    _journal_is_the_source(scene, figure)
    note(
        scene,
        MARGIN,
        enactment.NOTE_Y,
        CLOSING,
        width=CONTENT_WIDTH,
        colour=GAP.stroke,
    )
    return scene
