from __future__ import annotations

from excalidraw.layout import slide_header
from excalidraw.scene import Scene

from ..vocabulary import HEALTH, LIFECYCLE, PLACEMENT, PLUMBING, SCALING
from . import job_answers
from .job_answers import (
    ANSIBLE,
    BY_HAND,
    DNSMASQ,
    GOLEM,
    MONITORING,
    SRV_RECORDS,
    SYSTEMD,
    decided_then_enacted,
    mixture,
)

SLUG = "with-golem"
TITLE = "What lichess would use with golem"

SUBTITLE = "A person still chooses the host; golem installs the service there."

ANSWERS = {
    PLACEMENT: decided_then_enacted(
        BY_HAND.captioned("chooses the host"),
        GOLEM.captioned("installs it there"),
    ),
    LIFECYCLE: decided_then_enacted(
        GOLEM.captioned("enables and starts it"),
        SYSTEMD.captioned("keeps it running"),
    ),
    HEALTH: mixture(BY_HAND, MONITORING, SYSTEMD),
    PLUMBING: mixture(ANSIBLE, DNSMASQ, SRV_RECORDS),
    SCALING: mixture(BY_HAND),
}


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, TITLE, SUBTITLE)
    job_answers.draw(scene, ANSWERS, configuration_management=ANSIBLE)
    return scene
