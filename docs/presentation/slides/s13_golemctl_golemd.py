"""The routes, verbs and flags drawn here are quoted from the shipped code.

`VERB_CARDS`, `ROUTES` and `FLOW_STEPS` name real `golemctl` flags, real `golemd`
endpoints and the real `Observation` variants, so they go stale when those move.
Check against `sites/website/src/content/docs/reference/cli.mdx` and ADR 0058,
`docs/adr/0058-the-plan-reads-the-host-and-only-a-verdict-crosses-the-port.md`,
which is where the claim on the bar across the middle comes from.
"""

from __future__ import annotations

from typing import Sequence

from excalidraw.layout import TextLine, panel, slide_header, span_bar, text_card
from excalidraw.palette import (
    BLUE,
    BLUE_FILL,
    GOLEM,
    GREEN,
    GREEN_FILL,
    INK,
    INK_SOFT,
    TEAL,
    TEAL_FILL,
    VIOLET,
    VIOLET_FILL,
    Tone,
)
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.text import HAND, MONO

SLUG = "plan-before-apply"
TITLE = "Plan before apply"

CLIENT_TONE = Tone(BLUE, BLUE_FILL)
FLEET_TONE = Tone(VIOLET, VIOLET_FILL)
DAEMON_TONE = Tone(GREEN, GREEN_FILL)
WIRE_TONE = Tone(TEAL, TEAL_FILL)

HEADER_Y = MARGIN

PANEL_Y = 160.0
PANEL_HEIGHT = 520.0
CLIENT_PANEL_X = MARGIN
CLIENT_PANEL_WIDTH = 560.0
DAEMON_PANEL_X = 656.0
DAEMON_PANEL_WIDTH = 880.0

VERB_CARDS_Y = 224.0
VERB_GAP = 10.0

ROUTES_Y = 226.0
ROUTE_PITCH = 32.0
ROUTE_COLUMN_WIDTH = 300.0
ROUTE_GLOSS_X = 996.0
ROUTE_GLOSS_WIDTH = 520.0

HANDSHAKE_Y = 492.0
HANDSHAKE_HEIGHT = 60.0
HANDSHAKE_GAP = 30.0
HANDSHAKE_WIDTHS = (220.0, 300.0, 260.0)
HANDSHAKE_NOTE_Y = 570.0

VERDICT_BAR_Y = 700.0
VERDICT_BAR_HEIGHT = 48.0

FLOW_Y = 764.0
FLOW_HEIGHT = 120.0
FLOW_GAP = 36.0

CONFLICT_NOTE_Y = 900.0

CARD_PADDING = 12.0


def literal(body: str, size: float = 13) -> TextLine:
    return (body, size, MONO)


def gloss(body: str, size: float = 12) -> TextLine:
    return (body, size, HAND)


VERB_CARDS: tuple[tuple[Sequence[TextLine], Tone], ...] = (
    (
        (literal("golemctl apply <source> <addr>"), literal("--json   --reattach", 11)),
        CLIENT_TONE,
    ),
    (
        (
            literal("golemctl plan <source> <addr>"),
            literal("--json   --detail   --against-host", 11),
        ),
        CLIENT_TONE,
    ),
    ((literal("golemctl state <addr>"), literal("GET /state", 11)), CLIENT_TONE),
    ((literal("golemctl history <addr>"), literal("GET /revisions", 11)), CLIENT_TONE),
    (
        (literal("golemctl show <addr> <id>"), literal("GET /revisions/:id", 11)),
        CLIENT_TONE,
    ),
    (
        (
            literal("golemctl fleet apply | plan | status"),
            literal("--inventory <PATH>   --hosts <a,b>", 11),
            gloss("exactly three fleet verbs — no fleet state, history or show"),
        ),
        FLEET_TONE,
    ),
)

ROUTES: tuple[tuple[str, str], ...] = (
    ("POST /manifest", "apply a manifest; 202 with a reconcile id"),
    ("POST /plan?against_host=true", "plan only; the host read is opt-in"),
    ("GET /reconciles/latest", "follow the newest apply"),
    ("GET /reconciles/:id?after=<seq>", "poll one apply, resume from a sequence"),
    ("GET /state", "what golemd has applied"),
    ("GET /revisions", "the journal"),
    ("GET /revisions/:id", "one revision"),
    ("GET /status", "liveness"),
)

HANDSHAKE: tuple[str, ...] = (
    "POST /manifest",
    '202 {"reconcile_id": <u64>}',
    "GET /reconciles/:id",
)

FLOW_STEPS: tuple[tuple[float, Sequence[TextLine], Tone], ...] = (
    (
        300.0,
        (
            literal("golemctl plan --against-host"),
            gloss("ask the host before you touch it"),
        ),
        CLIENT_TONE,
    ),
    (
        300.0,
        (
            literal("POST /plan?against_host=true"),
            literal("PlanScope = JournalAndHost", 12),
            gloss("the host read is opt-in"),
        ),
        WIRE_TONE,
    ),
    (
        336.0,
        (
            literal("observe(&[GlyphOp]) -> Observations"),
            gloss("golemd probes dpkg, /etc and systemd on the host"),
        ),
        DAEMON_TONE,
    ),
    (
        428.0,
        (
            literal("Observation"),
            literal("Realized | Divergent | Absent | Unknown(Unknowable)", 12),
            literal("Unknowable = Sealed | Unreadable | NotModelled", 12),
            gloss("a verdict — never contents, mode, owner or dpkg status"),
        ),
        GOLEM,
    ),
)

CONFLICT_NOTE = (
    "409 HostBusy — an --against-host plan racing a live apply.        "
    "409 ReconcileInProgress — an apply racing an apply.        "
    "A plain golemctl plan still works during an apply."
)


def draw_verbs(scene: Scene) -> None:
    body = panel(
        scene,
        CLIENT_PANEL_X,
        PANEL_Y,
        CLIENT_PANEL_WIDTH,
        PANEL_HEIGHT,
        "On your machine — golemctl",
        tone=CLIENT_TONE,
    ).body
    cursor = VERB_CARDS_Y
    for lines, tone in VERB_CARDS:
        drawn = text_card(
            scene, body.x, cursor, body.width, lines, tone, padding=CARD_PADDING
        )
        cursor = drawn["y"] + drawn["height"] + VERB_GAP


def draw_routes(scene: Scene) -> None:
    body = panel(
        scene,
        DAEMON_PANEL_X,
        PANEL_Y,
        DAEMON_PANEL_WIDTH,
        PANEL_HEIGHT,
        "On the host — golemd, eight routes",
        tone=DAEMON_TONE,
    ).body
    for position, (route, caption) in enumerate(ROUTES):
        row_y = ROUTES_Y + position * ROUTE_PITCH
        scene.text(
            body.x,
            row_y,
            route,
            font_size=14,
            colour=INK,
            font_family=MONO,
            width=ROUTE_COLUMN_WIDTH,
        )
        scene.text(
            ROUTE_GLOSS_X,
            row_y + 1,
            caption,
            font_size=13,
            colour=INK_SOFT,
            width=ROUTE_GLOSS_WIDTH,
        )
    chip_x = body.x
    chips: list[dict] = []
    for width, body_text in zip(HANDSHAKE_WIDTHS, HANDSHAKE):
        chips.append(
            text_card(
                scene,
                chip_x,
                HANDSHAKE_Y,
                width,
                (literal(body_text),),
                WIRE_TONE,
                height=HANDSHAKE_HEIGHT,
                padding=CARD_PADDING,
                align="center",
            )
        )
        chip_x += width + HANDSHAKE_GAP
    middle = HANDSHAKE_Y + HANDSHAKE_HEIGHT / 2.0
    for position in range(len(chips) - 1):
        start = chips[position]["x"] + chips[position]["width"] + 4
        end = chips[position + 1]["x"] - 4
        scene.arrow([(start, middle), (end, middle)], stroke=INK_SOFT)
    scene.text(
        body.x,
        HANDSHAKE_NOTE_Y,
        "apply is a handshake: post the manifest, take the id, follow the stream.",
        font_size=14,
        colour=INK_SOFT,
        width=body.width,
    )


def draw_plan_flow(scene: Scene) -> None:
    cursor = MARGIN
    drawn: list[dict] = []
    for width, lines, tone in FLOW_STEPS:
        drawn.append(
            text_card(
                scene,
                cursor,
                FLOW_Y,
                width,
                lines,
                tone,
                height=FLOW_HEIGHT,
                padding=CARD_PADDING,
            )
        )
        cursor += width + FLOW_GAP
    middle = FLOW_Y + FLOW_HEIGHT / 2.0
    for position in range(len(drawn) - 1):
        start = drawn[position]["x"] + drawn[position]["width"] + 6
        end = drawn[position + 1]["x"] - 6
        scene.arrow([(start, middle), (end, middle)], stroke=INK_SOFT)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        "golemd, golemctl, and plan-before-apply",
        "Two binaries, one wire. Ask the host what an apply would do before you let it "
        "happen.",
        y=HEADER_Y,
    )
    draw_verbs(scene)
    draw_routes(scene)
    span_bar(
        scene,
        MARGIN,
        VERDICT_BAR_Y,
        CONTENT_WIDTH,
        "The plan reads the host — and only a verdict crosses the port.",
        tone=GOLEM,
        height=VERDICT_BAR_HEIGHT,
        font_size=17,
    )
    draw_plan_flow(scene)
    scene.text(
        MARGIN,
        CONFLICT_NOTE_Y,
        CONFLICT_NOTE,
        font_size=13,
        colour=INK_SOFT,
        font_family=MONO,
        width=CONTENT_WIDTH,
    )
    return scene
