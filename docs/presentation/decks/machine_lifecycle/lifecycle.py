from __future__ import annotations

from typing import Callable, NamedTuple, Sequence

from excalidraw import icons
from excalidraw.layout import LabelledBox, labelled_box
from excalidraw.palette import INK_FAINT, INK_SOFT, WHITE, Tone
from excalidraw.scene import CONTENT_WIDTH, LABEL_HEADROOM, MARGIN, Scene
from excalidraw.text import measured_height, measured_width, wrapped
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE

ID_NAMESPACE = "machine-lifecycle-band"

BAND_X = MARGIN
BAND_WIDTH = CONTENT_WIDTH
STEP_GAP = 14.0
STEP_WIDTH = (BAND_WIDTH - 4 * STEP_GAP) / 5.0

AXIS_Y = 248.0
NUMBER_SIZE = 42.0

STEP_Y = 296.0
STEP_HEIGHT = 340.0
STEP_PADDING = 20.0
MARK_SIZE = 104.0
MARK_TOP = STEP_Y + 44.0
NAME_TOP = MARK_TOP + MARK_SIZE + 24.0
NAME_BLOCK = 2 * BODY_SIZE * 1.25
GLOSS_TOP = NAME_TOP + NAME_BLOCK + 10.0

SPAN_Y = 676.0
SPAN_HEIGHT = 150.0

START_CAPTION = "an order at OVH"
END_CAPTION = "a machine in service"

MarkDrawer = Callable[[Scene, float, float, float, Tone], None]


def _machine_mark(scene: Scene, x: float, y: float, size: float, tone: Tone) -> None:
    icons.host(scene, x, y, size, tone=tone)


def _install_mark(scene: Scene, x: float, y: float, size: float, tone: Tone) -> None:
    icons.os_install(scene, x, y, size, tone=tone)


def _disks_mark(scene: Scene, x: float, y: float, size: float, tone: Tone) -> None:
    icons.disk_layout(scene, x, y, size, tone=tone)


def _playbook_mark(scene: Scene, x: float, y: float, size: float, tone: Tone) -> None:
    icons.source_file(scene, x, y, size, tone=tone)


def _services_mark(scene: Scene, x: float, y: float, size: float, tone: Tone) -> None:
    icons.service(scene, x, y, size, tone=tone, instance_tone=tone)


class Step(NamedTuple):
    number: int
    name: str
    gloss: str
    mark: MarkDrawer
    aspect: float


class Span(NamedTuple):
    first: int
    last: int
    keeper: str
    gloss: str
    tone: Tone
    by_hand: bool = False


STEPS: tuple[Step, ...] = (
    Step(1, "Order the machine", "at OVH", _machine_mark, icons.HOST_ASPECT),
    Step(
        2,
        "Install Debian",
        "the operating system",
        _install_mark,
        icons.OS_INSTALL_ASPECT,
    ),
    Step(
        3,
        "Partition the disks",
        "the disk layout",
        _disks_mark,
        icons.DISK_LAYOUT_ASPECT,
    ),
    Step(
        4,
        "Install the basics",
        "firewall, ssh, ntp, vrack",
        _playbook_mark,
        icons.SOURCE_FILE_ASPECT,
    ),
    Step(
        5,
        "Configure the services",
        "what the machine runs",
        _services_mark,
        icons.SERVICE_ASPECT,
    ),
)


def step_x(number: int) -> float:
    return BAND_X + (number - 1) * (STEP_WIDTH + STEP_GAP)


def step_centre_x(number: int) -> float:
    return step_x(number) + STEP_WIDTH / 2.0


def span_width(first: int, last: int) -> float:
    return (last - first + 1) * STEP_WIDTH + (last - first) * STEP_GAP


def draw_axis(scene: Scene) -> None:
    caption_top = AXIS_Y - NUMBER_SIZE / 2.0 - CAPTION_SIZE * 1.25 - 12.0
    scene.arrow(
        [(BAND_X, AXIS_Y), (BAND_X + BAND_WIDTH, AXIS_Y)],
        stroke=INK_SOFT,
        stroke_width=3,
    )
    scene.text(
        BAND_X,
        caption_top,
        START_CAPTION,
        font_size=CAPTION_SIZE,
        colour=INK_FAINT,
        width=measured_width(START_CAPTION, CAPTION_SIZE) + 8,
    )
    end_width = measured_width(END_CAPTION, CAPTION_SIZE) + 8
    scene.text(
        BAND_X + BAND_WIDTH - end_width,
        caption_top,
        END_CAPTION,
        font_size=CAPTION_SIZE,
        colour=INK_FAINT,
        align="right",
        width=end_width,
    )


def draw_steps(scene: Scene, tones: dict[int, Tone]) -> None:
    text_width = STEP_WIDTH - 2 * STEP_PADDING
    for step in STEPS:
        tone = tones[step.number]
        left = step_x(step.number)
        scene.rectangle(
            left,
            STEP_Y,
            STEP_WIDTH,
            STEP_HEIGHT,
            Tone(tone.stroke, WHITE, tone.stroke),
        )
        name = wrapped(step.name, text_width * LABEL_HEADROOM, BODY_SIZE)
        gloss = wrapped(step.gloss, text_width * LABEL_HEADROOM, CAPTION_SIZE)
        step.mark(
            scene,
            left + (STEP_WIDTH - step.aspect * MARK_SIZE) / 2.0,
            MARK_TOP,
            MARK_SIZE,
            tone,
        )
        scene.text(
            left + STEP_PADDING,
            NAME_TOP + (NAME_BLOCK - measured_height(name, BODY_SIZE)) / 2.0,
            name,
            font_size=BODY_SIZE,
            colour=tone.stroke,
            align="center",
            width=text_width,
        )
        scene.text(
            left + STEP_PADDING,
            GLOSS_TOP,
            gloss,
            font_size=CAPTION_SIZE,
            colour=INK_SOFT,
            align="center",
            width=text_width,
        )
    for step in STEPS:
        tone = tones[step.number]
        scene.rectangle(
            step_centre_x(step.number) - NUMBER_SIZE / 2.0,
            AXIS_Y - NUMBER_SIZE / 2.0,
            NUMBER_SIZE,
            NUMBER_SIZE,
            Tone(tone.stroke, WHITE, tone.stroke),
            label=str(step.number),
            label_font_size=BODY_SIZE,
        )


def draw_spans(scene: Scene, spans: Sequence[Span]) -> None:
    for span in spans:
        labelled_box(
            scene,
            step_x(span.first),
            SPAN_Y,
            span_width(span.first, span.last),
            SPAN_HEIGHT,
            LabelledBox(span.keeper, span.gloss, span.tone),
            align="center",
            stroke_style="dashed" if span.by_hand else "solid",
        )


def draw(scene: Scene, spans: Sequence[Span]) -> None:
    tones = {
        step.number: span.tone
        for span in spans
        for step in STEPS
        if span.first <= step.number <= span.last
    }
    draw_axis(scene)
    draw_steps(scene, tones)
    draw_spans(scene, spans)
