from __future__ import annotations

from excalidraw.layout import CoverageRow, coverage_bars, legend, note, slide_header
from excalidraw.palette import ANSIBLE, MANUAL
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene

SLUG = "ansible"
TITLE = "Where we were: Ansible"

BARS_Y = 210.0
BAR_HEIGHT = 76.0
BAR_GAP = 14.0
LABEL_WIDTH = 520.0

ANSIBLE_TAG = "Ansible reached it"
MANUAL_TAG = "manual"

ROWS = (
    CoverageRow("1. Core OS, network, security", 1.0, ANSIBLE, MANUAL, ANSIBLE_TAG),
    CoverageRow("2. Application hosting", 1.0, ANSIBLE, MANUAL, ANSIBLE_TAG),
    CoverageRow(
        "3. Connective infrastructure", 0.25, ANSIBLE, MANUAL, "", "mostly manual"
    ),
    CoverageRow("4. Tools, dependencies, runtimes", 1.0, ANSIBLE, MANUAL, ANSIBLE_TAG),
    CoverageRow("5. The applications", 0.0, ANSIBLE, MANUAL, "", MANUAL_TAG),
    CoverageRow(
        "6. Lifecycle / schedule / scaling",
        0.0,
        ANSIBLE,
        MANUAL,
        "",
        "manual — all five parts",
    ),
)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        "Where we were: Ansible",
        "How far a playbook actually reached, layer by layer.",
    )
    coverage_bars(
        scene,
        MARGIN,
        BARS_Y,
        CONTENT_WIDTH,
        ROWS,
        bar_height=BAR_HEIGHT,
        gap=BAR_GAP,
        label_width=LABEL_WIDTH,
    )
    legend(
        scene,
        MARGIN,
        BARS_Y + len(ROWS) * (BAR_HEIGHT + BAR_GAP) + 14,
        ((ANSIBLE, "a playbook could reach it"), (MANUAL, "a human decided and did it")),
    )
    note(
        scene,
        MARGIN,
        BARS_Y + len(ROWS) * (BAR_HEIGHT + BAR_GAP) + 70,
        "All five parts of orchestration lived in someone's head.",
        width=CONTENT_WIDTH,
    )
    return scene
