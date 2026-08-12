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
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE, HEADING_SIZE

SLUG = "where-it-broke"
TITLE = "Where it broke"

ROW_TOP = 196.0
ROW_HEIGHT = 118.0
ROW_GAP = 14.0
CLOSING_Y = 862.0


class Problem(NamedTuple):
    index_label: str
    title: str
    detail: str
    token: str = ""


PROBLEMS: tuple[Problem, ...] = (
    Problem(
        "1",
        "Ansible is imperative mutation",
        "idempotent by convention, not by construction",
    ),
    Problem("2", "No undo", "every rollback written by hand, as another play"),
    Problem(
        "3",
        "No real static analysis",
        "the dry run collapses, so runtime errors surface on a live host",
        "--check",
    ),
    Problem(
        "4",
        "No way to test against a known-good host",
        "nothing could answer what this change would do",
    ),
    Problem(
        "5",
        "Tied to the newest podman and Debian trixie",
        "the plumbing assumed the newest thing everywhere",
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
    padding: float = 18.0,
    index_gutter: float = 52.0,
) -> dict:
    rect = scene.rectangle(x, y, width, height, MANUAL)
    text_x = x + padding + index_gutter
    text_width = width - 2 * padding - index_gutter
    if problem.token:
        token_width = fit_width(problem.token, CAPTION_SIZE, font_family=MONO)
        token_height = CAPTION_SIZE * LINE_HEIGHT + 16
        scene.rectangle(
            x + width - padding - token_width,
            y + (height - token_height) / 2.0,
            token_width,
            token_height,
            Tone(MANUAL.stroke, WHITE, MANUAL.stroke),
            label=problem.token,
            label_font_size=CAPTION_SIZE,
            label_font_family=MONO,
        )
        text_width -= token_width + 24
    title = wrapped(problem.title, text_width * LABEL_HEADROOM, HEADING_SIZE)
    title_height = measured_height(title, HEADING_SIZE)
    detail = wrapped(problem.detail, text_width * LABEL_HEADROOM, BODY_SIZE)
    detail_height = measured_height(detail, BODY_SIZE)
    block_height = title_height + 8 + detail_height
    top = y + max(padding, (height - block_height) / 2.0)
    scene.text(
        text_x,
        top,
        title,
        font_size=HEADING_SIZE,
        colour=MANUAL.text,
        width=text_width,
    )
    scene.text(
        text_x,
        top + title_height + 8,
        detail,
        font_size=BODY_SIZE,
        colour=INK_SOFT,
        width=text_width,
    )
    scene.text(
        x + padding,
        y + (height - HEADING_SIZE * LINE_HEIGHT) / 2.0,
        problem.index_label,
        font_size=HEADING_SIZE,
        colour=MANUAL.stroke,
        width=index_gutter - 12,
    )
    return rect


def problem_stack(
    scene: Scene, x: float, y: float, width: float, problems: Sequence[Problem]
) -> list[dict]:
    return [
        problem_row(
            scene, x, y + position * (ROW_HEIGHT + ROW_GAP), width, ROW_HEIGHT, problem
        )
        for position, problem in enumerate(problems)
    ]


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(
        scene,
        "Where it broke",
        "Five problems, none of them a bug you could go and fix.",
    )
    problem_stack(scene, MARGIN, ROW_TOP, CONTENT_WIDTH, PROBLEMS)
    note(
        scene,
        MARGIN,
        CLOSING_Y,
        "The cost of writing changes as steps.",
        width=CONTENT_WIDTH,
    )
    return scene
