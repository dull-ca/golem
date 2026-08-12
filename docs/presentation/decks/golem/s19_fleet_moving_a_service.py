"""One service moving hosts, drawn on the fleet rather than as a sequence of steps.

The layer stripes do not change: a service is one unit inside layer 5, not a layer,
and emptying the losing machine's stripe would claim it stopped running everything.
The two badges and the machine outlines are the whole delta from the frame before.

The closing note is load-bearing and must keep saying what it says. golem ships no
cross-host ordering: `golemctl fleet` spawns one task per target with no barrier
between them (`apps/golemctl/src/fleet.rs`), and no ADR or TODO proposes otherwise.
What this frame claims over slide 12 is expressibility — three hand-sequenced edits
collapsing to one — and never an orchestrated cutover.
"""

from __future__ import annotations

from excalidraw.layout import badge, connector, note, slide_header, span_bar
from excalidraw.palette import ANSIBLE, GAP, GOLEM, INK_FAINT, WHITE, Tone
from excalidraw.scene import Scene
from excalidraw.type_scale import CAPTION_SIZE

from . import fleet

TOOLS = (
    fleet.Tool("Ansible", "layer 1, and installs golemd", ANSIBLE),
    fleet.Tool("emetc, golemctl", "compile the program, submit the manifest", GOLEM),
    fleet.Tool("golemd", "on every machine — layers 2 to 6", GOLEM),
)

SLUG = "fleet-moving-a-service"
TITLE = "The fleet: moving a service"

SUBTITLE = "One edit changes which machine a service belongs to. Both sides fall out of one manifest."

LOSING_MACHINE = 19
GAINING_MACHINE = 22

BADGE_Y = 762.0
BADGE_HEIGHT = 38.5
GOLEM_BAR_Y = 816.0
GOLEM_BAR_HEIGHT = 58.0
LIMIT_NOTE_Y = 886.0

LOSING_TONE = Tone(GAP.stroke, WHITE)
GAINING_TONE = Tone(GOLEM.stroke, WHITE)


def _annotate(scene: Scene, index: int, caption: str, tone: Tone) -> dict:
    centre_x = fleet.machine_centre_x(index)
    connector(
        scene,
        [(centre_x, fleet.FLEET_BOTTOM + 6.0), (centre_x, BADGE_Y - 6.0)],
        stroke=INK_FAINT,
        dashed=True,
    )
    return badge(
        scene,
        centre_x,
        BADGE_Y,
        caption,
        tone=tone,
        font_size=CAPTION_SIZE,
        anchor="center",
        height=BADGE_HEIGHT,
    )


def _machines() -> tuple[fleet.Machine, ...]:
    return tuple(
        fleet.Machine(
            fleet.EVERY_LAYER,
            outline=LOSING_TONE if index == LOSING_MACHINE else fleet.GOLEM_OUTLINE,
            agent=True,
        )
        for index in range(fleet.MACHINE_COUNT)
    )


def build() -> Scene:
    scene = Scene(SLUG, id_namespace=fleet.ID_NAMESPACE)
    slide_header(scene, TITLE, SUBTITLE)
    fleet.draw(scene, _machines())
    fleet.tool_column(scene, TOOLS)
    losing = _annotate(scene, LOSING_MACHINE, "removes it", LOSING_TONE)
    gaining = _annotate(scene, GAINING_MACHINE, "installs it", GAINING_TONE)
    middle = BADGE_Y + BADGE_HEIGHT / 2.0
    connector(
        scene,
        [
            (losing["x"] + losing["width"] + 8.0, middle),
            (gaining["x"] - 8.0, middle),
        ],
        stroke=INK_FAINT,
    )
    span_bar(
        scene,
        fleet.FLEET_X,
        GOLEM_BAR_Y,
        fleet.FLEET_WIDTH,
        "In golem: one edit, one apply, and the manifest names the machine.",
        tone=GOLEM,
        height=GOLEM_BAR_HEIGHT,
    )
    note(
        scene,
        fleet.FLEET_X,
        LIMIT_NOTE_Y,
        "Nothing orders the two, so both or neither may be running briefly.",
        width=fleet.FLEET_WIDTH,
        colour=GAP.stroke,
    )
    return scene
