"""Text measured without a font engine.

Nothing here loads a font, so every width is an estimate: a per-character advance
table for the hand-drawn font, a flat advance for the monospace one. The estimates
exist to decide line breaks and box sizes at build time, and they are tuned to err
wide — see `LABEL_HEADROOM` in scene.py for the slack that absorbs the error.
"""

from __future__ import annotations

HAND = 1
SANS = 2
MONO = 3

LINE_HEIGHT = 1.25

# NOTE: the font's true advance is about 0.62em. Over-measuring by a hair only
# widens a chip; under-measuring lets Excalidraw re-wrap a code literal on load,
# which is the visible bug. Err high on purpose.
MONOSPACE_ADVANCE = 0.65

_HAIRLINE = "il|!.,:;'`"
_NARROW = "jtfrI()[]{}/\\- "
_BROAD = "mwMW@%"


def character_advance(
    character: str, font_size: float, font_family: int = HAND
) -> float:
    if font_family == MONO:
        return MONOSPACE_ADVANCE * font_size
    if character in _HAIRLINE:
        return 0.30 * font_size
    if character in _NARROW:
        return 0.40 * font_size
    if character in _BROAD:
        return 0.94 * font_size
    if character.isupper():
        return 0.70 * font_size
    if character.isdigit():
        return 0.62 * font_size
    return 0.58 * font_size


def line_advance(line: str, font_size: float, font_family: int = HAND) -> float:
    return sum(
        character_advance(character, font_size, font_family) for character in line
    )


def measured_width(body: str, font_size: float, font_family: int = HAND) -> float:
    return max(
        (line_advance(line, font_size, font_family) for line in body.split("\n")),
        default=0.0,
    )


def measured_height(body: str, font_size: float, font_family: int = HAND) -> float:
    return len(body.split("\n")) * font_size * LINE_HEIGHT


def wrapped(
    body: str, available_width: float, font_size: float, font_family: int = HAND
) -> str:
    lines: list[str] = []
    for paragraph in body.split("\n"):
        current = ""
        for word in paragraph.split(" "):
            candidate = word if not current else f"{current} {word}"
            fits = line_advance(candidate, font_size, font_family) <= available_width
            if not current or fits:
                current = candidate
            else:
                lines.append(current)
                current = word
        lines.append(current)
    return "\n".join(lines)
