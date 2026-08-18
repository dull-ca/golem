from __future__ import annotations

from typing import NamedTuple, Sequence

from excalidraw.layout import connector
from excalidraw.palette import (
    ANSIBLE,
    GOLEM,
    INK_FAINT,
    INK_GHOST,
    INK_SOFT,
    ORANGE,
    ORANGE_FILL,
    WHITE,
    Tone,
)
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.text import MONO
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE

from ..lichess_fleet import HOSTS
from ..machines import Machine, cell_area, draw_machine, swatch_entry
from .glyph_ops import Op, op_legend

COLUMN_COUNT = 3
COLUMN_GAP = 46.0
COLUMN_WIDTH = (CONTENT_WIDTH - (COLUMN_COUNT - 1) * COLUMN_GAP) / COLUMN_COUNT

SHOWN_HOSTS: tuple[str, ...] = ("cobar", "dingo", "achoo")

BAND_Y = 186.0
HEADER_CLEARANCE = 12.0
BAND_PADDING = 18.0
BAND_NAME_WIDTH = 180.0
BAND_HEADING_WIDTH = 220.0
BAND_FIRST_ROW = 52.0
BAND_ROW_PITCH = 36.0
BAND_MARK = 28.0
BAND_TEXT_GAP = 12.0

ARROW_RUN = 44.0
MACHINE_HEIGHT = 200.0

CELL_COLUMNS = 4
CELL_ROWS = 2
CELL_GAP = 4.0
CELL_MARK = 40.0

JOURNAL_GAP = 42.0
JOURNAL_PADDING = 18.0
JOURNAL_FIRST_ROW = 46.0
JOURNAL_ROW_PITCH = 30.0
JOURNAL_TONE = Tone(ORANGE, WHITE)

LEGEND_Y = 812.0
LEGEND_CLEARANCE = 16.0
NOTE_Y = 858.0

PLAN_TONE = Tone(INK_FAINT, WHITE)
RECORD_TONE = Tone(ORANGE, ORANGE_FILL)

PRESENT_TONE = GOLEM
ABSENT_TONE = Tone(INK_GHOST, WHITE)

PRESENT_CAPTION = "golem keeps this"
ABSENT_CAPTION = "not on the host"


class Cell(NamedTuple):
    present: bool
    op: Op


class OpRow(NamedTuple):
    op: Op
    count: int
    slots: tuple[int, ...] = ()


class HostPanel(NamedTuple):
    name: str
    cells: tuple[Cell, ...]
    rows: tuple[OpRow, ...]
    revisions: int


class Band(NamedTuple):
    heading: str
    tone: Tone
    stroke_style: str
    arrows: bool
    marks_newest_revision: bool


PLAN = Band("plan", PLAN_TONE, "dashed", True, False)


def record(revision: int) -> Band:
    return Band(f"revision {revision}", RECORD_TONE, "solid", False, True)


def band_height(rows: int) -> float:
    return BAND_FIRST_ROW + rows * BAND_ROW_PITCH + 12.0


def journal_height(revisions: int) -> float:
    return JOURNAL_FIRST_ROW + revisions * JOURNAL_ROW_PITCH + 12.0


def revision_rows(count: int) -> tuple[str, ...]:
    return tuple(
        f"{number}  {'Init' if number == 1 else 'Reconcile'}"
        for number in range(1, count + 1)
    )


class Figure(NamedTuple):
    panels: tuple[HostPanel, ...]

    @property
    def band_bottom(self) -> float:
        return BAND_Y + band_height(max(len(panel.rows) for panel in self.panels))

    @property
    def machine_y(self) -> float:
        return self.band_bottom + ARROW_RUN

    @property
    def machine_bottom(self) -> float:
        return self.machine_y + MACHINE_HEIGHT

    @property
    def journal_y(self) -> float:
        return self.machine_bottom + JOURNAL_GAP

    @property
    def journal_bottom(self) -> float:
        return self.journal_y + journal_height(
            max(panel.revisions for panel in self.panels)
        )

    def column_x_of(self, name: str) -> float:
        return column_x([panel.name for panel in self.panels].index(name))

    def journal_row_y(self, revision: int) -> float:
        return (
            self.journal_y
            + JOURNAL_FIRST_ROW
            + (revision - 1) * JOURNAL_ROW_PITCH
            + CAPTION_SIZE * 1.25 / 2.0
        )

    def unit_cell_rect(self, x: float, slot: int) -> tuple[float, float, float, float]:
        left, top, width, height = cell_area(
            x, self.machine_y, COLUMN_WIDTH, MACHINE_HEIGHT, BODY_SIZE
        )
        cell_width = (width - (CELL_COLUMNS - 1) * CELL_GAP) / CELL_COLUMNS
        cell_height = (height - (CELL_ROWS - 1) * CELL_GAP) / CELL_ROWS
        return (
            left + (slot % CELL_COLUMNS) * (cell_width + CELL_GAP),
            top + (slot // CELL_COLUMNS) * (cell_height + CELL_GAP),
            cell_width,
            cell_height,
        )

    def slot_span(
        self, x: float, slots: Sequence[int]
    ) -> tuple[float, float, float, float]:
        rects = [self.unit_cell_rect(x, slot) for slot in slots]
        return (
            min(rect[0] for rect in rects),
            max(rect[0] + rect[2] for rect in rects),
            min(rect[1] for rect in rects),
            max(rect[1] + rect[3] for rect in rects),
        )


def units_on(name: str) -> int:
    return next(host.tool_units for host in HOSTS if host.name == name)


def column_x(index: int) -> float:
    return MARGIN + index * (COLUMN_WIDTH + COLUMN_GAP)


def present(count: int, op: Op) -> tuple[Cell, ...]:
    return tuple(Cell(True, op) for _ in range(count))


def absent(count: int, op: Op) -> tuple[Cell, ...]:
    return tuple(Cell(False, op) for _ in range(count))


def slots_from(first: int, count: int) -> tuple[int, ...]:
    return tuple(range(first, first + count))


def _check_geometry(figure: Figure, header_bottom: float) -> None:
    if header_bottom > BAND_Y - HEADER_CLEARANCE:
        raise ValueError(
            f"the header runs to y={header_bottom:.0f} and the operation band "
            f"starts at y={BAND_Y:.0f}: shorten the title or the subtitle"
        )
    if figure.journal_bottom > LEGEND_Y - LEGEND_CLEARANCE:
        raise ValueError(
            f"the journals run to y={figure.journal_bottom:.0f} and the legend "
            f"sits at y={LEGEND_Y:.0f}: drop a revision row or an operation row"
        )
    for panel in figure.panels:
        for row in panel.rows:
            if row.op.changes_cells and not row.slots:
                raise ValueError(
                    f"{panel.name}: a {row.op.verb} row names no unit slots, so "
                    f"nothing on the machine would be marked or pointed at"
                )


def _band(scene: Scene, figure: Figure, x: float, panel: HostPanel, band: Band) -> None:
    scene.rectangle(
        x,
        BAND_Y,
        COLUMN_WIDTH,
        band_height(len(panel.rows)),
        band.tone,
        stroke_style=band.stroke_style,
        stroke_width=2,
    )
    scene.text(
        x + BAND_PADDING,
        BAND_Y + 16,
        panel.name,
        font_size=CAPTION_SIZE,
        colour=INK_SOFT,
        font_family=MONO,
        width=BAND_NAME_WIDTH,
    )
    scene.text(
        x + COLUMN_WIDTH - BAND_PADDING - BAND_HEADING_WIDTH,
        BAND_Y + 12,
        band.heading,
        font_size=BODY_SIZE,
        colour=band.tone.stroke,
        align="right",
        width=BAND_HEADING_WIDTH,
    )
    for position, row in enumerate(panel.rows):
        top = BAND_Y + BAND_FIRST_ROW + position * BAND_ROW_PITCH
        row.op.mark(scene, x + BAND_PADDING, top, BAND_MARK, row.op.tone)
        scene.text(
            x + BAND_PADDING + BAND_MARK + BAND_TEXT_GAP,
            top + (BAND_MARK - CAPTION_SIZE * 1.25) / 2.0,
            row.op.units(row.count),
            font_size=CAPTION_SIZE,
            colour=row.op.tone.stroke,
            font_family=MONO,
            width=COLUMN_WIDTH - 2 * BAND_PADDING - BAND_MARK - BAND_TEXT_GAP,
        )


def _machine(scene: Scene, figure: Figure, x: float, panel: HostPanel) -> None:
    draw_machine(
        scene,
        x,
        figure.machine_y,
        Machine(panel.name, keeper=ANSIBLE, unit_tone=GOLEM, agent=True),
        width=COLUMN_WIDTH,
        height=MACHINE_HEIGHT,
        name_font_size=BODY_SIZE,
    )
    for slot, cell in enumerate(panel.cells):
        left, top, width, height = figure.unit_cell_rect(x, slot)
        scene.rectangle(
            left,
            top,
            width,
            height,
            PRESENT_TONE if cell.present else ABSENT_TONE,
            radius=False,
            stroke_width=2,
            stroke_style="solid" if cell.present else "dotted",
        )
        cell.op.mark(
            scene,
            left + (width - CELL_MARK) / 2.0,
            top + (height - CELL_MARK) / 2.0,
            CELL_MARK,
            cell.op.tone,
        )


def _journal(
    scene: Scene, figure: Figure, x: float, panel: HostPanel, band: Band
) -> None:
    top = figure.journal_y
    scene.rectangle(
        x, top, COLUMN_WIDTH, journal_height(panel.revisions), JOURNAL_TONE
    )
    scene.text(
        x + JOURNAL_PADDING,
        top + 14,
        "journal",
        font_size=CAPTION_SIZE,
        colour=ORANGE,
        width=BAND_NAME_WIDTH,
    )
    rows = revision_rows(panel.revisions)
    for position, row in enumerate(rows):
        newest = band.marks_newest_revision and position == len(rows) - 1
        scene.text(
            x + JOURNAL_PADDING,
            top + JOURNAL_FIRST_ROW + position * JOURNAL_ROW_PITCH,
            row,
            font_size=CAPTION_SIZE,
            colour=ORANGE if newest else INK_SOFT,
            font_family=MONO,
            width=COLUMN_WIDTH - 2 * JOURNAL_PADDING,
        )


def _arrows(scene: Scene, figure: Figure, x: float, panel: HostPanel) -> None:
    for row in panel.rows:
        if not row.op.changes_cells:
            continue
        left, right, top, _ = figure.slot_span(x, row.slots)
        middle = (left + right) / 2.0
        connector(
            scene,
            [(middle, figure.band_bottom + 8), (middle, top - 10)],
            stroke=row.op.tone.stroke,
            stroke_width=3,
        )


def legend(scene: Scene) -> None:
    cursor = op_legend(scene, MARGIN, LEGEND_Y)
    cursor = swatch_entry(
        scene, cursor, LEGEND_Y, PRESENT_TONE, "solid", PRESENT_CAPTION
    )
    swatch_entry(scene, cursor, LEGEND_Y, ABSENT_TONE, "dotted", ABSENT_CAPTION)


def draw(
    scene: Scene,
    panels: Sequence[HostPanel],
    band: Band,
    header_bottom: float,
) -> Figure:
    figure = Figure(tuple(panels))
    _check_geometry(figure, header_bottom)
    for index, panel in enumerate(figure.panels):
        x = column_x(index)
        _band(scene, figure, x, panel, band)
        _machine(scene, figure, x, panel)
        _journal(scene, figure, x, panel, band)
        if band.arrows:
            _arrows(scene, figure, x, panel)
    legend(scene)
    return figure
