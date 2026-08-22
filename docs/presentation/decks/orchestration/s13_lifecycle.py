from __future__ import annotations

from excalidraw import icons
from excalidraw.layout import (
    IconCard,
    StateNode,
    Transition,
    icon_card_row,
    note,
    slide_header,
    state_machine,
)
from excalidraw.palette import (
    GREEN,
    MANUAL,
    PENDING,
    SLATE,
    WHITE,
    YELLOW,
    Tone,
)
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import BODY_SIZE

from ..vocabulary import LIFECYCLE, part

SLUG = "lifecycle"
TITLE = part(LIFECYCLE).title

STATE_WIDTH = 250.0
STATE_HEIGHT = 90.0

RUNNING_TONE = Tone(GREEN, WHITE, GREEN)
STOPPED_TONE = Tone(SLATE, WHITE, SLATE)
DRAINING_TONE = Tone(YELLOW, WHITE, YELLOW)

STATES = (
    StateNode(
        "pending", "Pending", 100.0, 430.0, PENDING, STATE_WIDTH, STATE_HEIGHT,
        "not started yet",
    ),
    StateNode(
        "running", "Running", 620.0, 430.0, RUNNING_TONE, STATE_WIDTH, STATE_HEIGHT,
        "on a node",
    ),
    StateNode(
        "stopped", "Stopped", 1180.0, 430.0, STOPPED_TONE, STATE_WIDTH, STATE_HEIGHT,
        "asked to stop",
    ),
    StateNode(
        "draining", "Draining", 620.0, 210.0, DRAINING_TONE, STATE_WIDTH, STATE_HEIGHT,
        "leaving this node",
    ),
    StateNode(
        "failed", "Failed", 620.0, 650.0, MANUAL, STATE_WIDTH, STATE_HEIGHT,
        "exited badly",
    ),
)

MOVES = (
    Transition("pending", "running", "start"),
    Transition("running", "stopped", "stop", bow=-55.0),
    Transition("stopped", "running", "restart", bow=-55.0),
    Transition("running", "draining", "drain", bow=-55.0),
    Transition("running", "failed", "crash", bow=-55.0),
    Transition("draining", "pending", "rolling update", bow=60.0),
    Transition("failed", "pending", "rollback", bow=-60.0),
)

MOVE_CARDS_X = 1000.0
MOVE_CARDS_Y = 640.0
MOVE_CARD_WIDTH = 236.0
MOVE_CARD_HEIGHT = 240.0
MOVE_ICON_SIZE = 78.0
MOVE_CARD_GAP = 28.0

MOVE_CARDS = (
    IconCard(
        icons.drain,
        icons.DRAIN_ASPECT,
        "drain",
        "move work off first",
        Tone(YELLOW, WHITE, YELLOW),
    ),
    IconCard(
        icons.rollback,
        icons.ROLLBACK_ASPECT,
        "rollback",
        "back to the last good one",
        MANUAL,
    ),
)

CLOSING_Y = 890.0


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        f"The {TITLE.lower()} of one workload",
    )
    state_machine(scene, STATES, MOVES)
    icon_card_row(
        scene,
        MOVE_CARDS_X,
        MOVE_CARDS_Y,
        MOVE_CARDS,
        card_height=MOVE_CARD_HEIGHT,
        icon_size=MOVE_ICON_SIZE,
        card_width=MOVE_CARD_WIDTH,
        gap=MOVE_CARD_GAP,
        detail_font_size=BODY_SIZE,
    )
    note(
        scene,
        MARGIN,
        CLOSING_Y,
        "A platform performs these moves, or a person does.",
        width=CONTENT_WIDTH,
    )
    return scene
