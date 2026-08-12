from __future__ import annotations

from typing import NamedTuple, Sequence

from excalidraw.layout import LabelledBox, box_row, connector, note, panel, slide_header
from excalidraw.palette import (
    ANSIBLE,
    BESPOKE,
    GAP,
    INK_SOFT,
    NEUTRAL,
    PLATFORM,
    RED,
    SYSTEMD,
    THEIRS,
    TRANSPARENT,
    Tone,
)
from excalidraw.scene import (
    CONTENT_WIDTH,
    LABEL_HEADROOM,
    MARGIN,
    Scene,
    right_edge,
)
from excalidraw.text import HAND, MONO, measured_height, wrapped

SLUG = "december-plumbing"
TITLE = "December: the plumbing"

SECTION_TONE = Tone(INK_SOFT, TRANSPARENT, INK_SOFT)

DISCOVERY_PANEL_Y = 152
PLACEMENT_PANEL_Y = 392
PANEL_HEIGHT = 216
STEP_GAP = 48

MISSING_HEADING_Y = 636
MISSING_ROW_Y = 674
MISSING_ROW_HEIGHT = 100
MISSING_GAP = 32
CLOSING_NOTE_Y = 798


class Step(NamedTuple):
    title: str
    detail: str
    tone: Tone
    title_font_family: int = HAND


DISCOVERY: tuple[Step, ...] = (
    Step("OVH vrack", "a private L2 between the rented machines", THEIRS),
    Step(
        "dnsmasq",
        "one resolver per host,\nanswering for the private names",
        PLATFORM,
        MONO,
    ),
    Step("SRV records", "a name resolves to a host and a port", PLATFORM),
    Step("Clients", "resolve a service, never a machine", NEUTRAL),
)

PLACEMENT: tuple[Step, ...] = (
    Step(
        "hosts.py",
        "a hand-maintained placement table:\nwhich service runs on which host",
        BESPOKE,
        MONO,
    ),
    Step(
        "Ansible inventory and quadlet variables",
        "generated from that table",
        ANSIBLE,
    ),
    Step("systemd quadlets", "podman units written onto the host", SYSTEMD),
    Step("Lifecycle", "start, stop, restart —\nwhatever systemd gives you", SYSTEMD),
)

MISSING: tuple[LabelledBox, ...] = (
    LabelledBox("Drain", "nothing could move work off a host before taking it down", GAP),
    LabelledBox(
        "Move a service to another machine",
        "placement changed only by editing the table\nand re-running the play",
        GAP,
    ),
    LabelledBox("Roll back", "nothing recorded what the host looked like before", GAP),
)


def step_box(
    scene: Scene,
    x: float,
    y: float,
    width: float,
    height: float,
    step: Step,
    *,
    title_font_size: float = 17,
    detail_font_size: float = 12,
    padding: float = 14,
) -> dict:
    rect = scene.rectangle(x, y, width, height, step.tone)
    text_width = width - 2 * padding
    title = wrapped(step.title, text_width * LABEL_HEADROOM, title_font_size)
    title_height = measured_height(title, title_font_size)
    detail = (
        wrapped(step.detail, text_width * LABEL_HEADROOM, detail_font_size)
        if step.detail
        else ""
    )
    detail_height = measured_height(detail, detail_font_size) if detail else 0.0
    spacing = 8 if detail else 0
    block_height = title_height + spacing + detail_height
    top = y + max(padding, (height - block_height) / 2.0)
    scene.text(
        x + padding,
        top,
        title,
        font_size=title_font_size,
        colour=step.tone.text,
        align="center",
        width=text_width,
        font_family=step.title_font_family,
    )
    if detail:
        scene.text(
            x + padding,
            top + title_height + spacing,
            detail,
            font_size=detail_font_size,
            colour=INK_SOFT,
            align="center",
            width=text_width,
        )
    return rect


def flow_row(
    scene: Scene,
    x: float,
    y: float,
    width: float,
    height: float,
    steps: Sequence[Step],
    *,
    gap: float = STEP_GAP,
) -> list[dict]:
    box_width = (width - gap * (len(steps) - 1)) / len(steps)
    drawn = [
        step_box(scene, x + position * (box_width + gap), y, box_width, height, step)
        for position, step in enumerate(steps)
    ]
    middle = y + height / 2.0
    for position in range(len(drawn) - 1):
        connector(
            scene,
            [
                (right_edge(drawn[position]) + 8, middle),
                (drawn[position + 1]["x"] - 8, middle),
            ],
        )
    return drawn


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        "How December worked",
        "A private network, DNS that names services, and a placement table in Python.",
    )
    discovery = panel(
        scene,
        MARGIN,
        DISCOVERY_PANEL_Y,
        CONTENT_WIDTH,
        PANEL_HEIGHT,
        "Service discovery — a client resolves a service, not a machine",
        tone=SECTION_TONE,
        heading_font_size=20,
    )
    flow_row(
        scene,
        discovery.body.x,
        discovery.body.y,
        discovery.body.width,
        discovery.body.height,
        DISCOVERY,
    )
    placement = panel(
        scene,
        MARGIN,
        PLACEMENT_PANEL_Y,
        CONTENT_WIDTH,
        PANEL_HEIGHT,
        "Placement and lifecycle — a table in Python, expanded onto the host",
        tone=SECTION_TONE,
        heading_font_size=20,
    )
    flow_row(
        scene,
        placement.body.x,
        placement.body.y,
        placement.body.width,
        placement.body.height,
        PLACEMENT,
    )
    note(
        scene,
        MARGIN,
        MISSING_HEADING_Y,
        "What this could not do",
        width=CONTENT_WIDTH,
        font_size=20,
        colour=RED,
    )
    box_row(
        scene,
        MARGIN,
        MISSING_ROW_Y,
        MISSING,
        box_width=(CONTENT_WIDTH - 2 * MISSING_GAP) / 3.0,
        box_height=MISSING_ROW_HEIGHT,
        gap=MISSING_GAP,
        title_font_size=18,
        detail_font_size=13,
        align="center",
    )
    note(
        scene,
        MARGIN,
        CLOSING_NOTE_Y,
        "Placement lived in a Python file and in a human's head. Nothing on the host "
        "knew what it was supposed to look like.",
        width=CONTENT_WIDTH,
    )
    return scene
