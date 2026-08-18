"""The two terse facts recap slide 32; the panel is what earns this slide.

The panel heading is deliberately narrow. golem's own repository builds and
publishes the site's container image; it does not deploy that image anywhere.
The Emet program that reconciles the site onto the host lives in `dulliac`
(`fleet/main.emet`, `fleet/sites/Sites.emet`), a separate repository and golem's
first outside consumer. `examples/website/website.emet` in this repository is a
different site: it provisions `remora` for the self-hosted-CI demo loop and is
not the source for anything quoted below.
"""

from __future__ import annotations

from typing import Sequence

from excalidraw.layout import (
    LabelledBox,
    badge,
    labelled_box,
    PANEL_PADDING,
    panel,
    panel_height_for,
    slide_header,
)
from excalidraw.palette import GOLEM, INK, INK_SOFT, NEUTRAL, WHITE, Tone
from excalidraw.scene import CONTENT_WIDTH, LABEL_HEADROOM, MARGIN, Scene
from excalidraw.text import HAND, LINE_HEIGHT, MONO, measured_height, wrapped
from excalidraw.type_scale import BODY_SIZE, HEADING_SIZE

SLUG = "current-status"
TITLE = "Where golem is today"

FACTS_Y = 210.0
FACTS_HEIGHT = 92.0
FACTS_GAP = 40.0
FACT_WIDTH = (CONTENT_WIDTH - FACTS_GAP) / 2.0

PANEL_Y = 360.0
PANEL_HEADING = "The site that serves golem's documentation"
DOMAIN_CHIP = "golem.yyc.dev"
LINE_GAP = 14.0
LITERAL_INDENT = 28.0

TERSE_FACTS = (
    LabelledBox("Outcome-based", "", NEUTRAL),
    LabelledBox("No real review of the code", "", NEUTRAL),
)

EvidenceLine = tuple[str, float, int]

EVIDENCE: tuple[EvidenceLine, ...] = (
    (
        "An Emet program describes the site; golemd reconciles it on the host.",
        BODY_SIZE,
        HAND,
    ),
    ('scroll { name = "dull-01" }', BODY_SIZE, MONO),
    ('aptPackage { name = "podman" }', BODY_SIZE, MONO),
    (
        'file { path = "/etc/containers/systemd/golem-docs.container" }',
        BODY_SIZE,
        MONO,
    ),
    ('systemdService { unit = "golem-docs.service" }', BODY_SIZE, MONO),
    (
        "The program lives in dulliac, a separate repository and golem's first "
        "outside consumer.",
        BODY_SIZE,
        HAND,
    ),
    (
        "golem's own repository publishes the container image; it does not deploy it.",
        BODY_SIZE,
        HAND,
    ),
)


def laid_out(body: str, size: float, family: int, width: float) -> str:
    if family == MONO:
        return body
    return wrapped(body, width * LABEL_HEADROOM, size)


def evidence_height(width: float, lines: Sequence[EvidenceLine]) -> float:
    stacked = 0.0
    for body, size, family in lines:
        indent = LITERAL_INDENT if family == MONO else 0.0
        stacked += (
            measured_height(laid_out(body, size, family, width - indent), size)
            + LINE_GAP
        )
    return stacked - LINE_GAP


def draw_evidence(
    scene: Scene, x: float, y: float, width: float, lines: Sequence[EvidenceLine]
) -> float:
    cursor = y
    for body, size, family in lines:
        indent = LITERAL_INDENT if family == MONO else 0.0
        drawn = scene.text(
            x + indent,
            cursor,
            body,
            font_size=size,
            colour=INK if family == MONO else INK_SOFT,
            font_family=family,
            width=width - indent,
            wrap_width=None if family == MONO else width - indent,
        )
        cursor += drawn["height"] + LINE_GAP
    return cursor


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, TITLE)
    for position, fact in enumerate(TERSE_FACTS):
        labelled_box(
            scene,
            MARGIN + position * (FACT_WIDTH + FACTS_GAP),
            FACTS_Y,
            FACT_WIDTH,
            FACTS_HEIGHT,
            fact,
            title_font_size=HEADING_SIZE,
            align="center",
        )
    body_width = CONTENT_WIDTH - 2 * PANEL_PADDING
    area = panel(
        scene,
        MARGIN,
        PANEL_Y,
        CONTENT_WIDTH,
        panel_height_for(
            PANEL_HEADING, CONTENT_WIDTH, evidence_height(body_width, EVIDENCE)
        ),
        PANEL_HEADING,
        tone=GOLEM,
    )
    badge(
        scene,
        MARGIN + CONTENT_WIDTH - 22.0,
        PANEL_Y + 22.0,
        DOMAIN_CHIP,
        tone=Tone(GOLEM.stroke, WHITE, GOLEM.stroke),
        font_size=BODY_SIZE,
        anchor="right",
        height=BODY_SIZE * LINE_HEIGHT + 12.0,
        font_family=MONO,
    )
    draw_evidence(scene, area.body.x, area.body.y, area.body.width, EVIDENCE)
    return scene
