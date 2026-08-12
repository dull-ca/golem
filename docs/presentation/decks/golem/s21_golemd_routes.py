from __future__ import annotations

from excalidraw.layout import badge, note, slide_header
from excalidraw.palette import GAP, INK, INK_SOFT
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.text import HAND, MONO
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE

SLUG = "golemd-routes"
TITLE = "golemd — the routes"

ROUTES_Y = 200.0
ROUTE_PITCH = 62.0
ROUTE_WIDTH = 560.0
GLOSS_X = MARGIN + 600.0
GLOSS_WIDTH = 872.0

CONFLICT_HEADING_Y = 740.0
CONFLICT_Y = 782.0
CONFLICT_GAP = 28.0
CONFLICT_HEIGHT = 62.0
CONFLICT_CAPTION_Y = 856.0

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

CONFLICTS: tuple[tuple[str, str, float], ...] = (
    ("409 HostBusy", "a host-reading plan hit a live apply", 420.0),
    ("409 ReconcileInProgress", "an apply hit an apply", 560.0),
    ("plan still works", "a plain plan never blocks", 400.0),
)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        "golemd — on the host",
        "Eight routes, and two ways to be told to wait.",
    )
    for position, (route, caption) in enumerate(ROUTES):
        row_y = ROUTES_Y + position * ROUTE_PITCH
        scene.text(
            MARGIN,
            row_y,
            route,
            font_size=BODY_SIZE,
            colour=INK,
            font_family=MONO,
            width=ROUTE_WIDTH,
        )
        scene.text(
            GLOSS_X,
            row_y + 2,
            caption,
            font_size=BODY_SIZE,
            colour=INK_SOFT,
            width=GLOSS_WIDTH,
        )
    note(
        scene,
        MARGIN,
        CONFLICT_HEADING_Y,
        "When something else is already running:",
        width=CONTENT_WIDTH,
        colour=GAP.stroke,
    )
    cursor = MARGIN
    for body, caption, width in CONFLICTS:
        badge(
            scene,
            cursor,
            CONFLICT_Y,
            body,
            tone=GAP,
            font_size=BODY_SIZE,
            height=CONFLICT_HEIGHT,
            min_width=width,
            font_family=MONO if body.startswith("409") else HAND,
        )
        note(
            scene,
            cursor,
            CONFLICT_CAPTION_Y,
            caption,
            width=width,
            font_size=CAPTION_SIZE,
            align="center",
        )
        cursor += width + CONFLICT_GAP
    return scene
