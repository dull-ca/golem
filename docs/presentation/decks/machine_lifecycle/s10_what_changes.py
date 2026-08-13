"""Both halves at one geometry, so the difference is a shape rather than a claim.

Artifact, arrow, machine — twice. On the left the artifact slot is drawn empty,
in the dotted `INK_GHOST` the decks use everywhere for a thing that is not there
yet; on the right it holds a source file. A sentence saying "today there is no
file" would be arguable. An empty slot beside a full one is read before anyone
has finished the subtitle.
"""

from __future__ import annotations

from excalidraw import icons
from excalidraw.layout import PanelArea, connector, note, slide_header, split_compare
from excalidraw.palette import INK_GHOST, INK_SOFT, MANUAL, PULUMI, WHITE, Tone
from excalidraw.scene import CONTENT_WIDTH, MARGIN, Scene
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE

SLUG = "what-changes"
TITLE = "What changes about steps 1 to 3"

SUBTITLE = "The same three choices, and one artifact that does or does not exist."

COMPARISON_Y = 250.0
COMPARISON_HEIGHT = 430.0

ARTIFACT_WIDTH = 148.0
ARTIFACT_HEIGHT = 190.0
MACHINE_SIZE = 128.0
CAPTION_TOP = 252.0

NOTHING = Tone(INK_GHOST, WHITE, INK_SOFT)

CLOSING_Y = 730.0
CLOSING = (
    "Steps 4 and 5 do not move: Ansible keeps the basics, golem takes the services."
)


def _half(
    area: PanelArea,
    artifact_label: str,
    action: str,
    caption: str,
    tone: Tone,
    scene: Scene,
    *,
    drawn: bool,
) -> None:
    body = area.body
    top = body.y
    if drawn:
        icons.source_file(
            scene,
            body.x + (ARTIFACT_WIDTH - ARTIFACT_HEIGHT * icons.SOURCE_FILE_ASPECT) / 2.0,
            top,
            ARTIFACT_HEIGHT,
            tone=tone,
        )
    else:
        scene.rectangle(
            body.x,
            top,
            ARTIFACT_WIDTH,
            ARTIFACT_HEIGHT,
            NOTHING,
            stroke_style="dotted",
        )
    note(
        scene,
        body.x,
        top + ARTIFACT_HEIGHT + 14.0,
        artifact_label,
        width=ARTIFACT_WIDTH,
        font_size=CAPTION_SIZE,
        colour=INK_SOFT,
        align="center",
    )
    machine_width = MACHINE_SIZE * icons.HOST_ASPECT
    machine_x = body.x + body.width - machine_width
    icons.host(scene, machine_x, top + (ARTIFACT_HEIGHT - MACHINE_SIZE) / 2.0, MACHINE_SIZE)
    note(
        scene,
        machine_x,
        top + ARTIFACT_HEIGHT + 14.0,
        "the machine",
        width=machine_width,
        font_size=CAPTION_SIZE,
        colour=INK_SOFT,
        align="center",
    )
    connector(
        scene,
        [
            (body.x + ARTIFACT_WIDTH + 18.0, top + ARTIFACT_HEIGHT / 2.0),
            (machine_x - 18.0, top + ARTIFACT_HEIGHT / 2.0),
        ],
        stroke=tone.stroke,
        stroke_width=3,
        label=action,
    )
    note(
        scene,
        body.x,
        top + CAPTION_TOP,
        caption,
        width=body.width,
        font_size=BODY_SIZE,
    )


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, TITLE, SUBTITLE)
    left, right = split_compare(
        scene,
        MARGIN,
        COMPARISON_Y,
        CONTENT_WIDTH,
        COMPARISON_HEIGHT,
        ("Today", MANUAL),
        ("With Pulumi", PULUMI),
    )
    _half(
        left,
        "no file in the repository",
        "filled in at the panel",
        "The model, the release and the partition table are chosen in a browser, "
        "once per machine.",
        MANUAL,
        scene,
        drawn=False,
    )
    _half(
        right,
        "machines.ts",
        "reviewed, then applied",
        "The same three are fields in a file. Rebuilding the machine means running "
        "the program again.",
        PULUMI,
        scene,
        drawn=True,
    )
    note(scene, MARGIN, CLOSING_Y, CLOSING, width=CONTENT_WIDTH)
    return scene
