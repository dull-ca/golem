"""The six-rung ladder and its marker, drawn identically by slides 03 and 04.

03 is the ladder alone; 04 adds Portainer over the rungs it covers. The two have
to be pixel-identical so that flipping between them adds Portainer and changes
nothing else, which is why `draw()` takes no geometry and the constants below are
not parameters.
"""

from __future__ import annotations

from dataclasses import dataclass

from excalidraw.layout import Tick, badge, connector, timeline
from excalidraw.palette import INK_FAINT, PLATFORM, YOURS
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene, bottom_edge
from excalidraw.text import measured_height, wrapped
from excalidraw.type_scale import BODY_SIZE

TICKS = (
    Tick("Bare metal + config mgmt", tone=YOURS),
    Tick("Docker (one host)", tone=YOURS),
    Tick("Swarm", tone=PLATFORM),
    Tick("Nomad", tone=PLATFORM),
    Tick("Kubernetes", tone=PLATFORM),
    Tick("Managed Kubernetes", tone=PLATFORM),
)

LICHESS_RUNG = 0
PORTAINER_FIRST = 1
PORTAINER_LAST = 4

ANNOTATION_Y = 190.0
ANNOTATION_HEIGHT = 56.0
TIMELINE_Y = 292.0
MARKER_HEIGHT = 44.0
TICK_HEIGHT = 26.0

STEP = CONTENT_WIDTH / len(TICKS)
AXIS_Y = TIMELINE_Y + MARKER_HEIGHT
LABEL_TOP = AXIS_Y + TICK_HEIGHT / 2.0 + 14.0
LABEL_WIDTH = STEP - 18.0


@dataclass(frozen=True)
class Ladder:
    bottom: float

    @staticmethod
    def rung_x(rung: int) -> float:
        return MARGIN + STEP * (rung + 0.5)

    @staticmethod
    def rung_left(rung: int) -> float:
        return MARGIN + STEP * rung

    @staticmethod
    def rung_right(rung: int) -> float:
        return MARGIN + STEP * (rung + 1)


def _label_bottom() -> float:
    tallest = max(
        measured_height(
            wrapped(tick.label, LABEL_WIDTH * 0.88, BODY_SIZE), BODY_SIZE
        )
        for tick in TICKS
    )
    return LABEL_TOP + tallest


def draw(scene: Scene) -> Ladder:
    marker = badge(
        scene,
        Ladder.rung_x(LICHESS_RUNG),
        ANNOTATION_Y,
        "lichess is here",
        tone=YOURS,
        font_size=BODY_SIZE,
        anchor="center",
        height=ANNOTATION_HEIGHT,
    )
    connector(
        scene,
        [
            (Ladder.rung_x(LICHESS_RUNG), bottom_edge(marker) + 6),
            (Ladder.rung_x(LICHESS_RUNG), AXIS_Y - 18),
        ],
        stroke=INK_FAINT,
        dashed=True,
    )
    timeline(
        scene,
        MARGIN,
        TIMELINE_Y,
        CONTENT_WIDTH,
        TICKS,
        marker_height=MARKER_HEIGHT,
        tick_height=TICK_HEIGHT,
    )
    return Ladder(_label_bottom())
