"""The three facts that stop slide 07 from overstating what Pulumi covers.

Everything on this slide comes from the provider's own documentation, and each
line is checkable in the room:

- the order-cart resources exist only as data sources, so a plan code is read
  and a person chooses which one
- the two-hour delivery wait, and the error the apply ends in past it, are
  stated in `docs/resources/dedicated_server.md`
- provider v2.0.0 removed `ovh_me_installation_template` and its partition
  scheme resources and moved partitioning onto the server resource

Card one is not a defect. That a person decides which machine to buy is the same
position the decks take on placement: a choice golem deliberately does not make.
"""

from __future__ import annotations

from excalidraw.layout import LabelledBox, badge, box_row, slide_header
from excalidraw.palette import GAP, NEUTRAL, WHITE, YOURS, Tone
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import CAPTION_SIZE

SLUG = "where-a-person-stays"
TITLE = "What the resource does not remove"

SUBTITLE = "One decision that stays with a person, one failure mode, and one version fact."

CARDS_Y = 268.0
CARD_HEIGHT = 396.0
CARD_GAP = 34.0
CARD_WIDTH = (CONTENT_WIDTH - 2 * CARD_GAP) / 3.0
TAG_Y = CARDS_Y + CARD_HEIGHT + 20.0

CARDS = (
    (
        LabelledBox(
            "Choosing the machine",
            "Which model to buy stays a decision. Pulumi reads plan codes from OVH's "
            "order-cart data sources; a person picks one.",
            YOURS,
        ),
        "kept on purpose",
    ),
    (
        LabelledBox(
            "The delivery window",
            "The provider waits at most two hours for delivery. Past that the apply "
            "ends in error and OVH goes on delivering.",
            GAP,
        ),
        "the order is asynchronous",
    ),
    (
        LabelledBox(
            "The provider version",
            "Partitioning moved onto the server resource in provider v2.0.0, which "
            "removed the separate installation-template resources.",
            NEUTRAL,
        ),
        "v2.0.0 and later",
    ),
)


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, TITLE, SUBTITLE)
    box_row(
        scene,
        MARGIN,
        CARDS_Y,
        tuple(card for card, _ in CARDS),
        box_width=CARD_WIDTH,
        box_height=CARD_HEIGHT,
        gap=CARD_GAP,
        align="left",
    )
    for position, (card, tag) in enumerate(CARDS):
        badge(
            scene,
            MARGIN + position * (CARD_WIDTH + CARD_GAP) + CARD_WIDTH / 2.0,
            TAG_Y,
            tag,
            tone=Tone(card.tone.stroke, WHITE, card.tone.stroke),
            font_size=CAPTION_SIZE,
            anchor="center",
        )
    return scene
