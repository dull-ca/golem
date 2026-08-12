from __future__ import annotations

from typing import Sequence

from excalidraw.layout import TextLine, slide_header, text_card
from excalidraw.palette import TEAL, TEAL_FILL, Tone
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.text import HAND, MONO
from excalidraw.type_scale import BODY_SIZE

SLUG = "the-four-glyphs"
TITLE = "The four glyphs"

GLYPH_TONE = Tone(TEAL, TEAL_FILL)
CARDS_Y = 186.0
CARD_GAP = 16.0


def literal(body: str, size: float = BODY_SIZE) -> TextLine:
    return (body, size, MONO)


def gloss(body: str, size: float = BODY_SIZE) -> TextLine:
    return (body, size, HAND)


GLYPH_CARDS: tuple[Sequence[TextLine], ...] = (
    (
        literal("aptPackage { name }"),
        literal("Glyph::AptPackage { name }        key  apt:<name>"),
        gloss("a Debian package"),
    ),
    (
        literal("systemdService { unit }"),
        literal("Glyph::SystemdService { unit }        key  systemd:<unit>"),
        gloss("an enabled and started unit"),
    ),
    (
        literal("file { … }    directory { … }    symlink { … }"),
        literal("Glyph::Filesystem { path, entry: Entry }        key  file:<path>"),
        literal("Entry = File { contents, perms } | Directory { perms } | Symlink { target }"),
        literal("Perms { mode: u16, owner: Option<String>, group: Option<String> }"),
        gloss("one glyph, three surface spellings"),
    ),
    (
        literal("lineInFile { path, line }"),
        literal("Glyph::LineInFile { path, line }        key  fileline:<path>:<line>"),
        gloss("one line ensured present in a file"),
    ),
)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, "The four glyphs")
    cursor = CARDS_Y
    for lines in GLYPH_CARDS:
        drawn = text_card(scene, MARGIN, cursor, CONTENT_WIDTH, lines, GLYPH_TONE)
        cursor = drawn["y"] + drawn["height"] + CARD_GAP
    return scene
