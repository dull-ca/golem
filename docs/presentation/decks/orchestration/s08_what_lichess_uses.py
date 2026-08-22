from __future__ import annotations

from excalidraw.layout import slide_header
from excalidraw.scene import Scene

from ..vocabulary import HEALTH, LIFECYCLE, PLACEMENT, PLUMBING, SCALING
from . import job_answers
from .job_answers import (
    ANSIBLE,
    BY_HAND,
    MONITORING,
    SYSTEMD,
    mixture,
)

SLUG = "what-lichess-uses"
TITLE = "What lichess uses for each"

SUBTITLE = "Four of the five are done by hand."

ANSWERS = {
    PLACEMENT: mixture(BY_HAND),
    LIFECYCLE: mixture(SYSTEMD),
    HEALTH: mixture(BY_HAND, MONITORING, SYSTEMD),
    PLUMBING: mixture(BY_HAND),
    SCALING: mixture(BY_HAND),
}


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, TITLE, SUBTITLE)
    job_answers.draw(scene, ANSWERS, configuration_management=ANSIBLE)
    return scene
