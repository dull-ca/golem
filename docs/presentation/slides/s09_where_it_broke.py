from __future__ import annotations

from typing import NamedTuple, Sequence

from excalidraw.layout import note, slide_header
from excalidraw.palette import INK_SOFT, MANUAL, WHITE, Tone
from excalidraw.scene import (
    CONTENT_WIDTH,
    LABEL_HEADROOM,
    MARGIN,
    Scene,
    fit_width,
)
from excalidraw.text import LINE_HEIGHT, MONO, measured_height, wrapped

SLUG = "where-it-broke"
TITLE = "Where it broke"

ROW_TOP = 162
ROW_HEIGHT = 116
ROW_GAP = 14
CLOSING_NOTE_Y = 822


class Problem(NamedTuple):
    index_label: str
    title: str
    detail: str
    token: str = ""
    tone: Tone = MANUAL


PROBLEMS: tuple[Problem, ...] = (
    Problem(
        "1",
        "Ansible is imperative mutation",
        "idempotent by convention, not by construction",
    ),
    Problem(
        "2",
        "No undo",
        "every rollback had to be written by hand, as another play",
    ),
    Problem(
        "3",
        "No real static analysis",
        "the dry run collapses — a task that edits a file an earlier task created "
        "fails the whole run\nso you end up putting the change on a live host and "
        "finding the runtime errors there",
        "--check",
    ),
    Problem(
        "4",
        "Hard to test a change against a known-good state",
        "no way to ask what this change would do to a host that was already good",
    ),
    Problem(
        "5",
        "Dependent on the newest podman and Debian trixie",
        "the plumbing assumed the newest thing on every host",
    ),
)


def problem_row(
    scene: Scene,
    x: float,
    y: float,
    width: float,
    height: float,
    problem: Problem,
    *,
    index_font_size: float = 22,
    title_font_size: float = 20,
    detail_font_size: float = 14,
    token_font_size: float = 15,
    padding: float = 16,
    index_gutter: float = 46,
) -> dict:
    rect = scene.rectangle(x, y, width, height, problem.tone)
    text_x = x + padding + index_gutter
    text_width = width - 2 * padding - index_gutter
    if problem.token:
        token_width = fit_width(problem.token, token_font_size, font_family=MONO)
        token_height = token_font_size * LINE_HEIGHT + 12
        scene.rectangle(
            x + width - padding - token_width,
            y + (height - token_height) / 2.0,
            token_width,
            token_height,
            Tone(problem.tone.stroke, WHITE, problem.tone.stroke),
            label=problem.token,
            label_font_size=token_font_size,
            label_font_family=MONO,
        )
        text_width -= token_width + 20
    title = wrapped(problem.title, text_width * LABEL_HEADROOM, title_font_size)
    title_height = measured_height(title, title_font_size)
    detail = wrapped(problem.detail, text_width * LABEL_HEADROOM, detail_font_size)
    detail_height = measured_height(detail, detail_font_size)
    spacing = 6
    block_height = title_height + spacing + detail_height
    top = y + max(padding, (height - block_height) / 2.0)
    scene.text(
        text_x,
        top,
        title,
        font_size=title_font_size,
        colour=problem.tone.text,
        width=text_width,
    )
    scene.text(
        text_x,
        top + title_height + spacing,
        detail,
        font_size=detail_font_size,
        colour=INK_SOFT,
        width=text_width,
    )
    scene.text(
        x + padding,
        y + (height - index_font_size * LINE_HEIGHT) / 2.0,
        problem.index_label,
        font_size=index_font_size,
        colour=problem.tone.stroke,
        width=index_gutter - 10,
    )
    return rect


def problem_stack(
    scene: Scene,
    x: float,
    y: float,
    width: float,
    problems: Sequence[Problem],
    *,
    height: float = ROW_HEIGHT,
    gap: float = ROW_GAP,
) -> list[dict]:
    return [
        problem_row(
            scene, x, y + position * (height + gap), width, height, problem
        )
        for position, problem in enumerate(problems)
    ]


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        "Where it broke",
        "Five problems, and none of them a bug you could go and fix.",
    )
    problem_stack(scene, MARGIN, ROW_TOP, CONTENT_WIDTH, PROBLEMS)
    note(
        scene,
        MARGIN,
        CLOSING_NOTE_Y,
        "None of this is Ansible's fault. It is the cost of writing changes as steps "
        "instead of writing down the state you want.",
        width=CONTENT_WIDTH,
    )
    return scene
