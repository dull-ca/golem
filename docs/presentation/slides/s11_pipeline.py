from __future__ import annotations

from typing import Sequence

from excalidraw.layout import TextLine, panel, slide_header, text_card
from excalidraw.palette import (
    BLUE,
    BLUE_FILL,
    GREEN,
    GREEN_FILL,
    INK_SOFT,
    ORANGE,
    ORANGE_FILL,
    SLATE,
    SLATE_FILL,
    TEAL,
    TEAL_FILL,
    VIOLET,
    VIOLET_FILL,
    Tone,
)
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.text import HAND, MONO

SLUG = "the-pipeline"
TITLE = "The pipeline"

SOURCE_TONE = Tone(SLATE, SLATE_FILL)
COMPILER_TONE = Tone(VIOLET, VIOLET_FILL)
MANIFEST_TONE = Tone(TEAL, TEAL_FILL)
CLIENT_TONE = Tone(BLUE, BLUE_FILL)
DAEMON_TONE = Tone(GREEN, GREEN_FILL)
JOURNAL_TONE = Tone(ORANGE, ORANGE_FILL)

HEADER_Y = MARGIN

STAGE_Y = 160.0
STAGE_HEIGHT = 104.0
STAGE_WIDTH = 252.0
STAGE_GAP = 53.0

PANEL_Y = 296.0
PANEL_HEIGHT = 484.0
CARD_HEIGHT = 92.0
CARD_PITCH = 128.0
COLUMN_WIDTH = 700.0
COLUMN_GAP = 32.0
GRID_HEADROOM = 26.0
SNAKE_CLEARANCE = 20.0

FACTS_Y = 800.0


def literal(body: str, size: float = 14) -> TextLine:
    return (body, size, MONO)


def gloss(body: str, size: float = 13) -> TextLine:
    return (body, size, HAND)


STAGES: tuple[tuple[Sequence[TextLine], Tone], ...] = (
    ((literal("fleet.emet", 16), gloss("the program you author", 12)), SOURCE_TONE),
    (
        (literal("emetc build <source>", 16), gloss("one compile for the fleet", 12)),
        COMPILER_TONE,
    ),
    (
        (literal("manifest", 16), gloss("binary, content-addressed", 12)),
        MANIFEST_TONE,
    ),
    (
        (literal("golemctl apply", 16), literal("POST /manifest", 13)),
        CLIENT_TONE,
    ),
    ((literal("golemd", 16), gloss("on the host", 12)), DAEMON_TONE),
)

PLAN_CARDS: tuple[tuple[Sequence[TextLine], Tone], ...] = (
    (
        (
            literal("AddressedScroll { content_id, scroll }"),
            gloss("golemd selects this host's scroll by name"),
        ),
        MANIFEST_TONE,
    ),
    (
        (
            literal("reconcile::plan(prior, desired) -> Vec<GlyphOp>"),
            literal("Glyph::key()", 13),
            gloss("the diff is by content id — same id, no work"),
        ),
        CLIENT_TONE,
    ),
    (
        (
            literal("GlyphOp"),
            literal("Install | Remove | Replace | Noop", 13),
            gloss("four ops, and there is no fifth"),
        ),
        CLIENT_TONE,
    ),
)

ENACT_CARDS: tuple[tuple[Sequence[TextLine], Tone], ...] = (
    (
        (
            literal("Reconciler::apply(&Glyph, ContentId) -> Outcome"),
            literal("Outcome { op, cid, inverse, changed }", 13),
            gloss("apply captures the prior state as an Inverse, carried on the Outcome"),
        ),
        DAEMON_TONE,
    ),
    (
        (
            literal("Revision { id, created_at, kind, scroll_content_id, outcomes }"),
            literal("kind: RevisionKind = Init | Reconcile", 13),
            gloss("an append-only journal of what golem actually did"),
        ),
        JOURNAL_TONE,
    ),
    (
        (
            literal("Reconciler::reverse(&Outcome)"),
            gloss("replays that Outcome to restore the prior state exactly"),
        ),
        JOURNAL_TONE,
    ),
)

MANIFEST_FACTS: tuple[TextLine, ...] = (
    gloss("The manifest, exactly:"),
    literal(
        "Manifest { format_version, emet_version, scrolls: Vec<AddressedScroll> }"
        "        FORMAT_VERSION = 5",
        13,
    ),
    literal("AddressedScroll { content_id, scroll }", 13),
    literal(
        "ContentId = a 32-byte BLAKE3 digest over postcard bytes — one per scroll, one per glyph",
        13,
    ),
)


def stage_x(position: int) -> float:
    return MARGIN + position * (STAGE_WIDTH + STAGE_GAP)


def draw_stages(scene: Scene) -> None:
    for position, (lines, tone) in enumerate(STAGES):
        text_card(
            scene,
            stage_x(position),
            STAGE_Y,
            STAGE_WIDTH,
            lines,
            tone,
            height=STAGE_HEIGHT,
            align="center",
        )
    middle = STAGE_Y + STAGE_HEIGHT / 2.0
    for position in range(len(STAGES) - 1):
        start = stage_x(position) + STAGE_WIDTH + 8
        end = stage_x(position + 1) - 8
        scene.arrow([(start, middle), (end, middle)], stroke=INK_SOFT)
    descent_x = stage_x(len(STAGES) - 1) + STAGE_WIDTH / 2.0
    scene.arrow(
        [(descent_x, STAGE_Y + STAGE_HEIGHT), (descent_x, PANEL_Y)], stroke=INK_SOFT
    )


def draw_column(
    scene: Scene, x: float, top: float, cards: Sequence[tuple[Sequence[TextLine], Tone]]
) -> None:
    centre_x = x + COLUMN_WIDTH / 2.0
    for position, (lines, tone) in enumerate(cards):
        text_card(
            scene,
            x,
            top + position * CARD_PITCH,
            COLUMN_WIDTH,
            lines,
            tone,
            height=CARD_HEIGHT,
        )
    for position in range(len(cards) - 1):
        above = top + position * CARD_PITCH + CARD_HEIGHT + 6
        below = top + (position + 1) * CARD_PITCH - 6
        scene.arrow([(centre_x, above), (centre_x, below)], stroke=INK_SOFT)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        "The pipeline",
        "One program, compiled once. Every host is diffed by content id, and every "
        "edit records its own inverse.",
        y=HEADER_Y,
    )
    draw_stages(scene)

    body = panel(
        scene,
        MARGIN,
        PANEL_Y,
        CONTENT_WIDTH,
        PANEL_HEIGHT,
        "Inside golemd — one apply",
        tone=DAEMON_TONE,
    ).body
    grid_top = body.y + GRID_HEADROOM
    plan_x = body.x
    enact_x = body.x + COLUMN_WIDTH + COLUMN_GAP
    draw_column(scene, plan_x, grid_top, PLAN_CARDS)
    draw_column(scene, enact_x, grid_top, ENACT_CARDS)

    plan_centre = plan_x + COLUMN_WIDTH / 2.0
    enact_centre = enact_x + COLUMN_WIDTH / 2.0
    gutter_centre = plan_x + COLUMN_WIDTH + COLUMN_GAP / 2.0
    plan_bottom = grid_top + (len(PLAN_CARDS) - 1) * CARD_PITCH + CARD_HEIGHT
    scene.arrow(
        [
            (plan_centre, plan_bottom),
            (plan_centre, plan_bottom + SNAKE_CLEARANCE),
            (gutter_centre, plan_bottom + SNAKE_CLEARANCE),
            (gutter_centre, grid_top - SNAKE_CLEARANCE),
            (enact_centre, grid_top - SNAKE_CLEARANCE),
            (enact_centre, grid_top),
        ],
        stroke=INK_SOFT,
    )

    text_card(scene, MARGIN, FACTS_Y, CONTENT_WIDTH, MANIFEST_FACTS, MANIFEST_TONE)
    return scene
