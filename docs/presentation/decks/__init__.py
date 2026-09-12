"""Two decks, and the only place their running order is written down.

A deck package exposes `NAME`, `TITLE` and `SLIDE_MODULE_NAMES`; a slide module
exposes `SLUG`, `TITLE` and `build() -> Scene`. Everything else follows from
position in that tuple — the slide number, the `NN-slug.excalidraw` filename, and
the `NN · Title` frame name inside that deck's combined canvas. Reordering the
tuple renumbers the talk.

Strings that both decks have to agree on live in `vocabulary.py`, not in either
deck: the five names of the orchestration jobs are Dr. Dub's decomposition, and a
deck that renamed one would contradict the other.
"""

from __future__ import annotations

from importlib import import_module
from typing import Callable, NamedTuple

from excalidraw.scene import Scene

DECK_PACKAGE_NAMES: tuple[str, ...] = ("golem", "orchestration", "machine_lifecycle")


class Slide(NamedTuple):
    number: int
    slug: str
    title: str
    build: Callable[[], Scene]

    @property
    def filename(self) -> str:
        return f"{self.number:02d}-{self.slug}.excalidraw"

    @property
    def frame_name(self) -> str:
        return f"{self.number:02d} · {self.title}"


class Deck(NamedTuple):
    name: str
    title: str
    slides: tuple[Slide, ...]

    @property
    def directory(self) -> str:
        return self.name

    @property
    def combined_filename(self) -> str:
        return f"{self.name}-deck.excalidraw"

    @property
    def scene_key(self) -> str:
        return f"{self.name}-deck"


def _load_slide(package_name: str, number: int, module_name: str) -> Slide:
    module = import_module(f"{package_name}.{module_name}")
    return Slide(number, module.SLUG, module.TITLE, module.build)


def _load_deck(package_name: str) -> Deck:
    package = import_module(f"{__name__}.{package_name}")
    return Deck(
        package.NAME,
        package.TITLE,
        tuple(
            _load_slide(f"{__name__}.{package_name}", number, module_name)
            for number, module_name in enumerate(package.SLIDE_MODULE_NAMES, start=1)
        ),
    )


DECKS: tuple[Deck, ...] = tuple(_load_deck(name) for name in DECK_PACKAGE_NAMES)
