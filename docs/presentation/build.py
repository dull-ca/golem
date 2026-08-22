"""Generate the talk diagrams: one .excalidraw per slide, per deck, plus the decks.

    python docs/presentation/build.py [--out DIR]

Offline, stdlib only, and deterministic — rerunning it overwrites `dist/` with
byte-identical files, so a diff there is a real change. README.md covers the rest.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

PRESENTATION_ROOT = Path(__file__).resolve().parent
if str(PRESENTATION_ROOT) not in sys.path:
    sys.path.insert(0, str(PRESENTATION_ROOT))

from decks import DECKS, Deck
from excalidraw.scene import Scene, framed_deck, write_scene
from icon_sheet import ICON_SHEET_FILENAME, build_icon_sheet

DEFAULT_OUTPUT = PRESENTATION_ROOT / "dist"
DECK_COLUMNS = 3
DECK_GAP = 160.0


def build_deck(deck: Deck, output: Path) -> list[Path]:
    written: list[Path] = []
    named_scenes: list[tuple[str, Scene]] = []
    directory = output / deck.directory
    for slide in deck.slides:
        scene = slide.build()
        written.append(write_scene(directory / slide.filename, scene))
        named_scenes.append((slide.frame_name, scene))
    combined = framed_deck(
        named_scenes, key=deck.scene_key, columns=DECK_COLUMNS, gap=DECK_GAP
    )
    written.append(write_scene(directory / deck.combined_filename, combined))
    return written


def build_all(output: Path) -> list[Path]:
    written: list[Path] = []
    for deck in DECKS:
        written.extend(build_deck(deck, output))
    written.append(write_scene(output / ICON_SHEET_FILENAME, build_icon_sheet()))
    return written


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        prog="build.py", description="Generate the golem talk diagrams."
    )
    parser.add_argument("--out", type=Path, default=DEFAULT_OUTPUT)
    arguments = parser.parse_args(argv)
    for path in build_all(arguments.out):
        print(path)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
