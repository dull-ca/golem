from __future__ import annotations

from excalidraw.layout import slide_header
from excalidraw.scene import Scene

from ..vocabulary import HEALTH, LIFECYCLE, PLACEMENT, PLUMBING, SCALING
from . import job_answers
from .job_answers import (
    ANSIBLE,
    BY_HAND,
    DNSMASQ,
    MONITORING,
    SRV_RECORDS,
    SYSTEMD,
    decided_then_enacted,
    mixture,
)

SLUG = "the-december-plan"
TITLE = "What lichess planned in December"

SUBTITLE = "A person still chooses the host; Ansible installs the service there."

ANSWERS = {
    PLACEMENT: decided_then_enacted(
        BY_HAND.captioned("chooses the host"),
        ANSIBLE.captioned("installs it there"),
    ),
    LIFECYCLE: mixture(SYSTEMD),
    HEALTH: mixture(BY_HAND, MONITORING, SYSTEMD),
    PLUMBING: mixture(ANSIBLE, DNSMASQ, SRV_RECORDS),
    SCALING: mixture(BY_HAND),
}


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, TITLE, SUBTITLE)
    job_answers.draw(scene, ANSWERS, configuration_management=ANSIBLE)
    return scene
