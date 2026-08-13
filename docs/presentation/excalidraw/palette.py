"""Excalidraw's own palette hexes, and the meanings the slides colour by.

The hex constants are the editor's default swatches, so a hand-edited shape picks
up the same colour from the toolbar. Above them sit the `Tone`s the slides name —
`YOURS`, `PLATFORM`, `ANSIBLE`, `GAP`, and the icon meanings `NODE`, `WORKLOAD`,
`IMAGE`, `CONTROL`, `STORE` — so a slide says who answers a layer, or what a mark
stands for, and the colour follows. Recolouring either deck happens here, not in
forty slide modules.
"""

from __future__ import annotations

from typing import NamedTuple

TRANSPARENT = "transparent"
WHITE = "#ffffff"
PAPER = "#f8f9fa"

INK = "#1e1e1e"
INK_SOFT = "#495057"
INK_FAINT = "#868e96"
INK_GHOST = "#ced4da"

RED = "#e03131"
RED_FILL = "#ffc9c9"
GREEN = "#2f9e44"
GREEN_FILL = "#b2f2bb"
BLUE = "#1971c2"
BLUE_FILL = "#a5d8ff"
YELLOW = "#f08c00"
YELLOW_FILL = "#ffec99"
VIOLET = "#6741d9"
VIOLET_FILL = "#d0bfff"
ORANGE = "#e8590c"
ORANGE_FILL = "#ffd8a8"
TEAL = "#0c8599"
TEAL_FILL = "#99e9f2"
GRAPE = "#9c36b5"
GRAPE_FILL = "#eebefa"
SLATE = "#495057"
SLATE_FILL = "#e9ecef"


class Tone(NamedTuple):
    stroke: str
    fill: str
    text: str = INK


NEUTRAL = Tone(INK_FAINT, PAPER, INK_SOFT)
OUTLINE = Tone(INK, TRANSPARENT, INK)

YOURS = Tone(YELLOW, YELLOW_FILL)
THEIRS = Tone(INK_FAINT, SLATE_FILL, INK_SOFT)
HOSTED = Tone(BLUE, BLUE_FILL)

PLATFORM = Tone(BLUE, BLUE_FILL)
CONTAINER = Tone(TEAL, TEAL_FILL)
ANSIBLE = Tone(GRAPE, GRAPE_FILL)
PULUMI = Tone(BLUE, BLUE_FILL)
BESPOKE = Tone(ORANGE, ORANGE_FILL)
SYSTEMD = Tone(VIOLET, VIOLET_FILL)
GOLEM = Tone(GREEN, GREEN_FILL)
GAP = Tone(RED, RED_FILL)
MANUAL = Tone(RED, WHITE, RED)

NODE = Tone(SLATE, SLATE_FILL)
WORKLOAD = Tone(TEAL, TEAL_FILL)
IMAGE = Tone(VIOLET, VIOLET_FILL)
CONTROL = Tone(BLUE, BLUE_FILL)
WIRE = Tone(BLUE, BLUE_FILL)
STORE = Tone(GRAPE, GRAPE_FILL)
PENDING = Tone(ORANGE, WHITE, ORANGE)
HEALTHY = Tone(GREEN, GREEN_FILL)
