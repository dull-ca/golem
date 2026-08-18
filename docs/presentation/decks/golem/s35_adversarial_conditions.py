"""Refused, retried and absorbed are the reconciler's own result variants.

Not a rhetorical split: refused is `EnactError::Fatal`, retried is
`EnactError::Retryable` (`apps/golemd/src/reconciler.rs`), and absorbed is `Ok`
-- `apply` succeeds, so nothing is retried and no recorded inverse puts the
prior state back.
A slide that only listed what golem handles well would not be a caveat; the
third column is why this one exists. Each case below is commented with its
source in `apps/golemd/src/reconcilers.rs`, so a later editor can check the
claim against the code instead of the slide alone.
"""

from __future__ import annotations

from typing import NamedTuple, Sequence

from excalidraw.layout import (
    PANEL_PADDING,
    Area,
    badge,
    note,
    panel,
    panel_height_for,
    slide_header,
)
from excalidraw.palette import GAP, INK_SOFT, OUTLINE, WHITE, Tone
from excalidraw.scene import CONTENT_WIDTH, LABEL_HEADROOM, MARGIN, Scene, bottom_edge
from excalidraw.text import MONO, measured_height, wrapped
from excalidraw.type_scale import BODY_SIZE, CAPTION_SIZE

SLUG = "adversarial-conditions"
TITLE = "When things fail"
SUBTITLE = (
    "The reconcilers are exercised against tempfiles and a fake command runner; "
    "the end-to-end install-to-decommission run against a real Debian box is "
    "deferred."
)

PANELS_Y = 210.0
PANEL_GAP = 40.0
PANEL_WIDTH = (CONTENT_WIDTH - 2 * PANEL_GAP) / 3.0

CHIP_HEIGHT = CAPTION_SIZE * 1.25 + 12.0
CHIP_GAP = 14.0
GLOSS_GAP = 16.0
ITEM_GAP = 16.0


class Group(NamedTuple):
    heading: str
    result_variant: str
    gloss: str
    cases: Sequence[str]
    tone: Tone


GROUPS: tuple[Group, ...] = (
    # NOTE: Fatal is defined and never retried at reconciler.rs:14-20,
    # foreman.rs:2410.
    Group(
        "Refused",
        "Fatal",
        "never retried",
        (
            "a file already where a directory is declared",  # reconcilers.rs:959-961
            "a file already where a symlink is declared",  # reconcilers.rs:1004-1006
            "a symlink already pointing somewhere else",  # reconcilers.rs:999-1002
            "a prior file that is not UTF-8",  # reconcilers.rs:1033-1034 (read_file)
        ),
        OUTLINE,
    ),
    # NOTE: max_attempts=5, on_exhaust=Rollback at config.rs:91-102.
    Group(
        "Retried",
        "Retryable",
        "five attempts by default, then the unit rolls back",
        (
            # every syscall arm maps io::Error to Retryable:
            # reconcilers.rs:975, 1022, 1230-1232, 1266, 1281-1295, 1307
            "a read-only filesystem, or permission denied",
            "a directory where a file is expected",  # reconcilers.rs:1036-1038 (EISDIR via fs::read)
            "the dpkg lock held by another process",  # reconcilers.rs:125-137, 363-372
            # module doc reconcilers.rs:30-35; enacted at :257-259, :329-334, :446-452
            "a unit latched failed, its start limit burnt",
        ),
        OUTLINE,
    ),
    Group(
        "Absorbed",
        "Ok",
        "no failure reported, and no record that puts the prior state back",
        (
            # read_file (reconcilers.rs:1029-1041) follows the link via fs::read
            # and gets ENOENT, so `prior` is None; write_file_atomic
            # (:1276-1296) renames a regular file over the link; apply_file
            # (:909-918) records Inverse::DeleteFile, which deletes the file on
            # reverse and never restores the link.
            "a broken symlink at a declared file path is replaced; reverse "
            "deletes the file and never restores the link",
            # observe_perms (reconcilers.rs:1042-1055) captures mode, owner and
            # group only; `grep -rn "xattr|selinux|acl" apps/golemd/` finds nothing.
            "ACLs, extended attributes and SELinux labels are neither observed "
            "nor restored",
        ),
        GAP,
    ),
)


BODY_WIDTH = PANEL_WIDTH - 2 * PANEL_PADDING


def stacked_height(bodies: Sequence[str], font_size: float, gap: float) -> float:
    if not bodies:
        return 0.0
    laid_out = (
        measured_height(wrapped(body, BODY_WIDTH * LABEL_HEADROOM, font_size), font_size)
        for body in bodies
    )
    return sum(laid_out) + gap * (len(bodies) - 1)


def cases_offset() -> float:
    tallest_gloss = max(
        stacked_height([group.gloss], CAPTION_SIZE, 0.0) for group in GROUPS
    )
    return CHIP_HEIGHT + CHIP_GAP + tallest_gloss + GLOSS_GAP


def panels_height() -> float:
    tallest_cases = max(
        stacked_height(group.cases, BODY_SIZE, ITEM_GAP) for group in GROUPS
    )
    return panel_height_for(
        GROUPS[0].heading, PANEL_WIDTH, cases_offset() + tallest_cases
    )


def draw_heading(scene: Scene, x: float, height: float, group: Group) -> Area:
    area = panel(
        scene, x, PANELS_Y, PANEL_WIDTH, height, group.heading, tone=group.tone
    ).body
    badge(
        scene,
        area.x,
        area.y,
        group.result_variant,
        tone=Tone(group.tone.stroke, WHITE, group.tone.stroke),
        font_size=CAPTION_SIZE,
        height=CHIP_HEIGHT,
        font_family=MONO,
    )
    gloss = note(
        scene,
        area.x,
        area.y + CHIP_HEIGHT + CHIP_GAP,
        group.gloss,
        width=area.width,
        font_size=CAPTION_SIZE,
        colour=INK_SOFT,
    )
    return Area(area.x, bottom_edge(gloss), area.width, area.height)


def draw_cases(scene: Scene, area: Area, y: float, cases: Sequence[str]) -> None:
    cursor = y
    for case in cases:
        drawn = note(
            scene, area.x, cursor, case, width=area.width, font_size=BODY_SIZE
        )
        cursor = bottom_edge(drawn) + ITEM_GAP


def build() -> Scene:
    scene = Scene(SLUG)
    slide_header(scene, TITLE, SUBTITLE)
    height = panels_height()
    headed = [
        draw_heading(
            scene, MARGIN + position * (PANEL_WIDTH + PANEL_GAP), height, group
        )
        for position, group in enumerate(GROUPS)
    ]
    cases_y = max(area.y for area in headed) + GLOSS_GAP
    for area, group in zip(headed, GROUPS):
        draw_cases(scene, area, cases_y, group.cases)
    return scene
