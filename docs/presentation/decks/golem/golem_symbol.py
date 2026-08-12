from __future__ import annotations

from excalidraw.assets import EmbeddedImage, vendored_svg

FILENAME = "robot-golem.svg"
CREDIT = "Robot golem icon by Lorc, game-icons.net, licensed CC BY 3.0."


def mark() -> EmbeddedImage:
    return vendored_svg(FILENAME)
