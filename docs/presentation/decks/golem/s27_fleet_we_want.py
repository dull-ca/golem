"""The fleet with every unit kept by golem — an intention, never a report.

Slide 26 draws the fleet today. This frame is one flip away from it: the same
geometry, the same tool chips, `units_all_tool_kept` in place of `units_split`,
so every recorded unit goes green and nothing is left dashed. On its own, out of
sequence or with the title missed, that drawing reads as a claim that golem
already keeps the fleet. The coverage rows below are what stop it.

The cell key is one entry, in the header beside the title, and it belongs there
rather than under the fleet where slide 26 puts its legend. A lone green swatch
in the left column lands below the work key's layer-5 swatch — the same green at
the same size in the same column — and reads as a seventh kind of work. One
entry is all there is to gloss: *a unit kept by hand* and *nobody has it written
down* describe nothing on this canvas.
"""

from __future__ import annotations

from excalidraw.layout import CoverageRow, coverage_bars, slide_header
from excalidraw.palette import ANSIBLE, GOLEM
from excalidraw.scene import CONTENT_RIGHT, MARGIN, Scene
from excalidraw.text import LINE_HEIGHT, measured_width
from excalidraw.type_scale import CAPTION_SIZE, TITLE_SIZE

from ..lichess_fleet import (
    HAND_UNIT_COUNT,
    HOST_COUNT,
    TOOL_KEPT_HOSTS,
    TOOL_UNIT_COUNT,
    UNIT_COUNT,
)
from ..machines import (
    FLEET_WIDTH,
    FLEET_X,
    LEGEND_SWATCH,
    LEGEND_Y,
    draw_fleet,
    swatch_entry,
)
from . import fleet

SLUG = "fleet-we-want"
TITLE = "The fleet we want: every unit kept by golem"

MACHINES_KEPT_TODAY = len(TOOL_KEPT_HOSTS)

SUBTITLE = (
    "golemd runs on every machine that carries a unit. "
    f"Today golem keeps units on {MACHINES_KEPT_TODAY} of the {HOST_COUNT} machines."
)

CELL_CAPTION = "a unit golem keeps"
CAPTION_BOX_SLACK = 8.0
CELL_KEY_WIDTH = (
    LEGEND_SWATCH * 1.4
    + 12.0
    + measured_width(CELL_CAPTION, CAPTION_SIZE)
    + CAPTION_BOX_SLACK
)
CELL_KEY_X = CONTENT_RIGHT - CELL_KEY_WIDTH
CELL_KEY_Y = MARGIN + (TITLE_SIZE * LINE_HEIGHT - LEGEND_SWATCH) / 2.0

COVERAGE_BAR_HEIGHT = 40.0
COVERAGE_GAP = 14.0
COVERAGE_LABEL_WIDTH = 360.0

# NOTE: the second row is the whole of this slide's honesty, and it is not
# decoration. A full green track above a short green stub and a wide red
# remainder is wrong-looking before a word of it is read, and it prints what
# golem keeps today beside the whole inventory drawn above. Both rows count off
# `lichess_fleet`, so neither can drift. Dropping the row, or its
# `remainder_tag`, turns an aspiration into a false claim about today.
#
# One reference class per region: the subtitle counts machines, these bars count
# units, the cell key counts nothing. Harmonising the subtitle and the bars onto
# a single unit of account would put one numeral on the frame twice with two
# different referents.
COVERAGE = (
    CoverageRow(
        "the fleet we want",
        1.0,
        GOLEM,
        covered_tag=f"all {UNIT_COUNT} units",
    ),
    CoverageRow(
        "the fleet today",
        TOOL_UNIT_COUNT / UNIT_COUNT,
        GOLEM,
        covered_tag=f"{TOOL_UNIT_COUNT} units",
        remainder_tag=f"{HAND_UNIT_COUNT} units still by hand",
    ),
)

TOOLS = (
    fleet.Tool("Ansible", "the core OS, network and security", ANSIBLE, work=(1,)),
    fleet.Tool("emetc, golemctl", "compile, then submit the manifest", GOLEM),
    fleet.Tool("golemd", "on the host, keeping its own units", GOLEM, work=(2, 3, 4, 5, 6)),
)


def build() -> Scene:
    scene = Scene(SLUG, id_namespace=fleet.ID_NAMESPACE)
    fleet.check_header(slide_header(scene, TITLE, SUBTITLE))
    swatch_entry(scene, CELL_KEY_X, CELL_KEY_Y, GOLEM, "solid", CELL_CAPTION)
    fleet.work_key(scene)
    draw_fleet(scene, fleet.units_all_tool_kept(GOLEM))
    fleet.tool_column(scene, TOOLS)
    coverage_bars(
        scene,
        FLEET_X,
        LEGEND_Y,
        FLEET_WIDTH,
        COVERAGE,
        bar_height=COVERAGE_BAR_HEIGHT,
        gap=COVERAGE_GAP,
        label_width=COVERAGE_LABEL_WIDTH,
    )
    return scene
