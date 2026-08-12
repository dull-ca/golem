"""The running order, and the only place it is written down.

A slide module exposes three names — `SLUG`, `TITLE`, and `build() -> Scene` —
and appears once in `SLIDE_MODULE_NAMES`. Everything else follows from its
position in that tuple: the slide number, the `NN-slug.excalidraw` filename, and
the `NN · Title` frame name in the deck. Reordering the tuple renumbers the talk.
"""

from __future__ import annotations

from importlib import import_module
from typing import Callable, NamedTuple

from excalidraw.scene import Scene

SLIDE_MODULE_NAMES: tuple[str, ...] = (
    "s01_what_you_buy",
    "s02_what_you_configure",
    "s03_lichess_stack",
    "s04_orchestration",
    "s05_bought_orchestration",
    "s06_ansible",
    "s07_december_containers",
    "s08_december_plumbing",
    "s09_where_it_broke",
    "s10_what_golem_is",
    "s11_pipeline",
    "s12_emet_glyphs",
    "s13_golemctl_golemd",
)


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


def _load(number: int, module_name: str) -> Slide:
    module = import_module(f"{__name__}.{module_name}")
    return Slide(number, module.SLUG, module.TITLE, module.build)


SLIDES: tuple[Slide, ...] = tuple(
    _load(number, module_name)
    for number, module_name in enumerate(SLIDE_MODULE_NAMES, start=1)
)
