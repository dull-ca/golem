from __future__ import annotations

from typing import Callable, NamedTuple

from excalidraw.palette import INK_FAINT, INK_GHOST, TEAL, WHITE, Tone
from excalidraw.scene import LABEL_HEADROOM, Scene
from excalidraw.text import MONO, measured_height, wrapped
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE, HEADING_SIZE

KindDrawer = Callable[..., None]

GLYPH_TONE = Tone(TEAL, WHITE)
GHOST_TONE = Tone(INK_GHOST, WHITE)
LEG_STROKE = INK_FAINT


def apt_package(
    scene: Scene, x: float, y: float, size: float, *, tone: Tone = GLYPH_TONE
) -> None:
    scene.rectangle(
        x, y + 0.10 * size, size, 0.80 * size, tone, radius=False, stroke_width=2
    )
    scene.line(
        [(x, y + 0.36 * size), (x + size, y + 0.36 * size)],
        stroke=tone.stroke,
        stroke_width=2,
    )
    scene.line(
        [(x + 0.50 * size, y + 0.10 * size), (x + 0.50 * size, y + 0.36 * size)],
        stroke=tone.stroke,
        stroke_width=2,
    )


def systemd_service(
    scene: Scene, x: float, y: float, size: float, *, tone: Tone = GLYPH_TONE
) -> None:
    scene.ellipse(
        x + 0.12 * size,
        y + 0.22 * size,
        0.76 * size,
        0.76 * size,
        Tone(tone.stroke, WHITE),
        stroke_width=2,
    )
    scene.line(
        [(x + 0.50 * size, y + 0.06 * size), (x + 0.50 * size, y + 0.46 * size)],
        stroke=tone.stroke,
        stroke_width=3,
    )


def filesystem_entry(
    scene: Scene, x: float, y: float, size: float, *, tone: Tone = GLYPH_TONE
) -> None:
    scene.rectangle(
        x,
        y + 0.16 * size,
        0.42 * size,
        0.16 * size,
        tone,
        radius=False,
        stroke_width=2,
    )
    scene.rectangle(
        x + 0.30 * size,
        y + 0.04 * size,
        0.36 * size,
        0.32 * size,
        Tone(tone.stroke, WHITE),
        radius=False,
        stroke_width=2,
    )
    scene.rectangle(
        x, y + 0.30 * size, size, 0.56 * size, tone, radius=False, stroke_width=2
    )


def line_in_file(
    scene: Scene, x: float, y: float, size: float, *, tone: Tone = GLYPH_TONE
) -> None:
    scene.rectangle(
        x + 0.12 * size,
        y + 0.06 * size,
        0.76 * size,
        0.88 * size,
        Tone(tone.stroke, WHITE),
        radius=False,
        stroke_width=2,
    )
    for row in (0.28, 0.44, 0.80):
        scene.line(
            [(x + 0.24 * size, y + row * size), (x + 0.70 * size, y + row * size)],
            stroke=tone.stroke,
            stroke_width=1,
        )
    scene.line(
        [(x + 0.18 * size, y + 0.62 * size), (x + 0.86 * size, y + 0.62 * size)],
        stroke=tone.stroke,
        stroke_width=4,
    )


class Kind(NamedTuple):
    key: str
    draw: KindDrawer
    spellings: tuple[str, ...]
    gloss: str

    @property
    def spelling_block(self) -> str:
        return "\n".join(self.spellings)


APT = Kind("apt", apt_package, ("aptPackage",), "a Debian package")
SYSTEMD = Kind(
    "systemd", systemd_service, ("systemdService",), "an enabled and started unit"
)
FILESYSTEM = Kind(
    "filesystem",
    filesystem_entry,
    ("file", "directory", "symlink"),
    "one entry at one path",
)
LINE = Kind("line", line_in_file, ("lineInFile",), "one line ensured present in a file")

KINDS: tuple[Kind, ...] = (APT, SYSTEMD, FILESYSTEM, LINE)

ELLIPSIS = "…"
ELLIPSIS_CAPTION = "more glyphs, the same four kinds"

TILE_WIDTH = 280.0
TILE_GAP = 20.0
TILE_PADDING = 18.0
GHOST_WIDTH = 240.0
ICON_SIZE = 100.0
ICON_GAP = 16.0
SPELLING_GAP = 10.0

ROW_WIDTH = len(KINDS) * TILE_WIDTH + len(KINDS) * TILE_GAP + GHOST_WIDTH
TEXT_WIDTH = TILE_WIDTH - 2 * TILE_PADDING


def _gloss_lines(kind: Kind) -> str:
    return wrapped(kind.gloss, TEXT_WIDTH * LABEL_HEADROOM, CAPTION_SIZE)


def _caption_block(kind: Kind) -> float:
    return (
        measured_height(kind.spelling_block, BODY_SIZE)
        + SPELLING_GAP
        + measured_height(_gloss_lines(kind), CAPTION_SIZE)
    )


TILE_HEIGHT = (
    2 * TILE_PADDING
    + ICON_SIZE
    + ICON_GAP
    + max(_caption_block(kind) for kind in KINDS)
)


class Fan(NamedTuple):
    legs: tuple[dict, ...]
    ghost_leg: dict
    tiles: tuple[dict, ...]

    @property
    def bottom(self) -> float:
        return max(tile["y"] + tile["height"] for tile in self.tiles)


def tile_x(row_x: float, position: int) -> float:
    return row_x + position * (TILE_WIDTH + TILE_GAP)


def ghost_x(row_x: float) -> float:
    return row_x + len(KINDS) * (TILE_WIDTH + TILE_GAP)


def _tile(scene: Scene, x: float, y: float, kind: Kind) -> dict:
    rect = scene.rectangle(x, y, TILE_WIDTH, TILE_HEIGHT, GLYPH_TONE, stroke_width=2)
    text_width = TEXT_WIDTH
    kind.draw(scene, x + (TILE_WIDTH - ICON_SIZE) / 2.0, y + TILE_PADDING, ICON_SIZE)
    spelling_y = y + TILE_PADDING + ICON_SIZE + ICON_GAP
    scene.text(
        x + TILE_PADDING,
        spelling_y,
        kind.spelling_block,
        font_size=BODY_SIZE,
        colour=GLYPH_TONE.stroke,
        align="center",
        font_family=MONO,
        width=text_width,
    )
    gloss = _gloss_lines(kind)
    scene.text(
        x + TILE_PADDING,
        spelling_y
        + measured_height(kind.spelling_block, BODY_SIZE)
        + SPELLING_GAP,
        gloss,
        font_size=CAPTION_SIZE,
        colour=INK_FAINT,
        align="center",
        width=text_width,
    )
    return rect


def draw_fan(
    scene: Scene, origin_x: float, origin_y: float, row_x: float, row_y: float
) -> Fan:
    legs = tuple(
        scene.line(
            [(origin_x, origin_y), (tile_x(row_x, position) + TILE_WIDTH / 2.0, row_y)],
            stroke=LEG_STROKE,
            stroke_width=2,
        )
        for position in range(len(KINDS))
    )
    ghost_leg = scene.line(
        [(origin_x, origin_y), (ghost_x(row_x) + GHOST_WIDTH / 2.0, row_y)],
        stroke=GHOST_TONE.stroke,
        stroke_width=2,
        stroke_style="dotted",
    )
    tiles = tuple(
        _tile(scene, tile_x(row_x, position), row_y, kind)
        for position, kind in enumerate(KINDS)
    )
    left = ghost_x(row_x)
    scene.text(
        left,
        row_y + TILE_PADDING,
        ELLIPSIS,
        font_size=HEADING_SIZE,
        colour=GHOST_TONE.stroke,
        align="center",
        width=GHOST_WIDTH,
    )
    scene.text(
        left,
        row_y + TILE_PADDING + HEADING_SIZE * 1.25 + SPELLING_GAP,
        wrapped(ELLIPSIS_CAPTION, GHOST_WIDTH * LABEL_HEADROOM, CAPTION_SIZE),
        font_size=CAPTION_SIZE,
        colour=GHOST_TONE.stroke,
        align="center",
        width=GHOST_WIDTH,
    )
    return Fan(legs, ghost_leg, tiles)
