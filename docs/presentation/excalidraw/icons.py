"""Composable marks drawn from rectangles, ellipses and lines. No assets, no text.

Every icon takes a `Scene`, an origin and a `size`, draws into the scene, and
returns a `Mark` holding the elements it added and the box it declared. `size` is
the mark's height; its width is `size` times the icon's `*_ASPECT` constant, which
callers need *before* drawing to centre a mark in a card — hence a module constant
per icon rather than a field read off the result.

Nothing here draws text, so no icon can breach the type floor at any scale, and a
mark stays legible when it shrinks. `test_scenes.py` asserts both, plus that every
element an icon draws lands inside the box the icon declared.

The marks compose: `cluster` is `host` repeated inside a dashed enclosure,
`binding` is `pending_workload` over three `host` marks, `replica_set` is
`container` repeated. That is why a container looks the same everywhere it
appears in either deck — there is one `container`, and everything else calls it.
"""

from __future__ import annotations

from typing import Callable, NamedTuple, Sequence

from .palette import (
    CONTROL,
    GAP,
    HEALTHY,
    IMAGE,
    INK_FAINT,
    INK_SOFT,
    NODE,
    PENDING,
    RED,
    STORE,
    TRANSPARENT,
    WHITE,
    WIRE,
    WORKLOAD,
    Tone,
)
from .scene import Scene

OUTLINE_ONLY = Tone(INK_SOFT, TRANSPARENT, INK_SOFT)


class Mark(NamedTuple):
    x: float
    y: float
    width: float
    height: float
    elements: tuple[dict, ...]

    @property
    def right(self) -> float:
        return self.x + self.width

    @property
    def bottom(self) -> float:
        return self.y + self.height

    @property
    def centre_x(self) -> float:
        return self.x + self.width / 2.0

    @property
    def centre_y(self) -> float:
        return self.y + self.height / 2.0


def _captured(
    scene: Scene, first: int, x: float, y: float, width: float, height: float
) -> Mark:
    return Mark(x, y, width, height, tuple(scene.elements[first:]))


CONTAINER_ASPECT = 1.30


def container(
    scene: Scene,
    x: float,
    y: float,
    size: float,
    *,
    tone: Tone = WORKLOAD,
    stroke_style: str = "solid",
) -> Mark:
    first = len(scene.elements)
    width = CONTAINER_ASPECT * size
    scene.rectangle(x, y, width, size, tone, stroke_style=stroke_style)
    scene.line(
        [(x + 0.04 * size, y + 0.26 * size), (x + width - 0.04 * size, y + 0.26 * size)],
        stroke=tone.stroke,
        stroke_width=1,
    )
    for fraction in (0.30, 0.50, 0.70):
        rib_x = x + fraction * width
        scene.line(
            [(rib_x, y + 0.40 * size), (rib_x, y + 0.86 * size)],
            stroke=tone.stroke,
            stroke_width=1,
        )
    return _captured(scene, first, x, y, width, size)


CONTAINER_IMAGE_ASPECT = 1.45


def container_image(
    scene: Scene, x: float, y: float, size: float, *, tone: Tone = IMAGE
) -> Mark:
    first = len(scene.elements)
    slab_width = 1.30 * size
    for offset_x, offset_y in ((0.15, 0.0), (0.075, 0.35), (0.0, 0.70)):
        scene.rectangle(
            x + offset_x * size,
            y + offset_y * size,
            slab_width,
            0.30 * size,
            tone,
        )
    return _captured(scene, first, x, y, CONTAINER_IMAGE_ASPECT * size, size)


REGISTRY_ASPECT = 1.25


def registry(
    scene: Scene, x: float, y: float, size: float, *, tone: Tone = STORE
) -> Mark:
    first = len(scene.elements)
    width = REGISTRY_ASPECT * size
    scene.rectangle(x, y + 0.30 * size, width, 0.70 * size, tone, radius=False)
    scene.line(
        [
            (x, y + 0.30 * size),
            (x + width / 2.0, y),
            (x + width, y + 0.30 * size),
        ],
        stroke=tone.stroke,
    )
    for row in (0.46, 0.66, 0.86):
        scene.line(
            [
                (x + 0.16 * width, y + row * size),
                (x + 0.84 * width, y + row * size),
            ],
            stroke=tone.stroke,
            stroke_width=1,
        )
    return _captured(scene, first, x, y, width, size)


HOST_ASPECT = 1.35


def host(
    scene: Scene,
    x: float,
    y: float,
    size: float,
    *,
    tone: Tone = NODE,
    stroke_style: str = "solid",
) -> Mark:
    first = len(scene.elements)
    width = HOST_ASPECT * size
    scene.rectangle(x, y, width, 0.82 * size, tone, stroke_style=stroke_style)
    for row in (0.26, 0.48):
        scene.line(
            [
                (x + 0.12 * width, y + row * size),
                (x + 0.70 * width, y + row * size),
            ],
            stroke=tone.stroke,
            stroke_width=1,
        )
    scene.ellipse(
        x + 0.80 * width,
        y + 0.22 * size,
        0.12 * size,
        0.12 * size,
        Tone(tone.stroke, tone.stroke),
    )
    scene.rectangle(
        x + 0.20 * width, y + 0.86 * size, 0.60 * width, 0.14 * size, tone, radius=False
    )
    return _captured(scene, first, x, y, width, size)


CLUSTER_ASPECT = 3.10


def cluster(
    scene: Scene,
    x: float,
    y: float,
    size: float,
    *,
    tone: Tone = NODE,
    members: int = 3,
) -> Mark:
    first = len(scene.elements)
    width = CLUSTER_ASPECT * size
    scene.rectangle(x, y, width, size, OUTLINE_ONLY, stroke_style="dashed")
    member_size = 0.62 * size
    member_width = HOST_ASPECT * member_size
    gap = (width - 0.24 * size - members * member_width) / max(members - 1, 1)
    for index in range(members):
        host(
            scene,
            x + 0.12 * size + index * (member_width + gap),
            y + (size - member_size) / 2.0,
            member_size,
            tone=tone,
        )
    return _captured(scene, first, x, y, width, size)


SCHEDULER_ASPECT = 1.60


def scheduler(
    scene: Scene, x: float, y: float, size: float, *, tone: Tone = CONTROL
) -> Mark:
    first = len(scene.elements)
    width = SCHEDULER_ASPECT * size
    scene.diamond(x, y + 0.14 * size, 0.90 * size, 0.72 * size, tone)
    for row, style in ((0.14, "dashed"), (0.50, "solid"), (0.86, "dashed")):
        scene.arrow(
            [(x + 0.92 * size, y + 0.50 * size), (x + width, y + row * size)],
            stroke=tone.stroke,
            stroke_style=style,
        )
    return _captured(scene, first, x, y, width, size)


PENDING_WORKLOAD_ASPECT = CONTAINER_ASPECT


def pending_workload(
    scene: Scene, x: float, y: float, size: float, *, tone: Tone = PENDING
) -> Mark:
    first = len(scene.elements)
    container(scene, x, y, size, tone=tone, stroke_style="dashed")
    return _captured(scene, first, x, y, PENDING_WORKLOAD_ASPECT * size, size)


BINDING_ASPECT = 2.60


# NOTE: assignment has to read as an act, not a noun — an unplaced workload, the
# nodes that could have taken it, and the one arrow that settled it. The rejected
# candidates stay on the mark, drawn faint and dashed, because a binding with the
# alternatives erased is just a container sitting on a box.
def binding(
    scene: Scene,
    x: float,
    y: float,
    size: float,
    *,
    chosen: int = 1,
    candidates: int = 3,
    workload_tone: Tone = PENDING,
    node_tone: Tone = NODE,
) -> Mark:
    first = len(scene.elements)
    width = BINDING_ASPECT * size
    workload_size = 0.40 * size
    workload_width = CONTAINER_ASPECT * workload_size
    workload_x = x + (width - workload_width) / 2.0
    pending_workload(scene, workload_x, y, workload_size, tone=workload_tone)
    node_size = 0.42 * size
    node_width = HOST_ASPECT * node_size
    span = (width - node_width) / max(candidates - 1, 1)
    node_top = y + 0.58 * size
    for index in range(candidates):
        node_x = x + index * span
        host(scene, node_x, node_top, node_size, tone=node_tone)
        settled = index == chosen
        scene.arrow(
            [
                (workload_x + workload_width / 2.0, y + workload_size + 0.02 * size),
                (node_x + node_width / 2.0, node_top - 0.02 * size),
            ],
            stroke=workload_tone.stroke if settled else INK_FAINT,
            stroke_width=2 if settled else 1,
            stroke_style="solid" if settled else "dashed",
        )
    return _captured(scene, first, x, y, width, size)


HEALTH_PROBE_ASPECT = 1.60


def health_probe(
    scene: Scene, x: float, y: float, size: float, *, tone: Tone = HEALTHY
) -> Mark:
    first = len(scene.elements)
    width = HEALTH_PROBE_ASPECT * size
    scene.rectangle(x, y + 0.14 * size, width, 0.72 * size, tone)
    scene.line(
        [
            (x + 0.10 * width, y + 0.50 * size),
            (x + 0.30 * width, y + 0.50 * size),
            (x + 0.40 * width, y + 0.24 * size),
            (x + 0.52 * width, y + 0.76 * size),
            (x + 0.64 * width, y + 0.42 * size),
            (x + 0.74 * width, y + 0.50 * size),
            (x + 0.90 * width, y + 0.50 * size),
        ],
        stroke=tone.stroke,
    )
    return _captured(scene, first, x, y, width, size)


DRIFT_ASPECT = 1.50


def drift(
    scene: Scene,
    x: float,
    y: float,
    size: float,
    *,
    desired_tone: Tone = HEALTHY,
    actual_tone: Tone = GAP,
) -> Mark:
    first = len(scene.elements)
    width = DRIFT_ASPECT * size
    plate_width = 1.15 * size
    scene.rectangle(x, y, plate_width, 0.72 * size, desired_tone)
    scene.rectangle(
        x + width - plate_width,
        y + 0.28 * size,
        plate_width,
        0.72 * size,
        actual_tone,
        stroke_style="dashed",
    )
    return _captured(scene, first, x, y, width, size)


NETWORK_LINK_ASPECT = 2.00


def network_link(
    scene: Scene, x: float, y: float, size: float, *, tone: Tone = WIRE
) -> Mark:
    first = len(scene.elements)
    width = NETWORK_LINK_ASPECT * size
    endpoint = 0.70 * size
    scene.ellipse(x, y + 0.15 * size, endpoint, endpoint, tone)
    scene.ellipse(x + width - endpoint, y + 0.15 * size, endpoint, endpoint, tone)
    scene.line(
        [
            (x + endpoint, y + 0.50 * size),
            (x + width - endpoint, y + 0.50 * size),
        ],
        stroke=tone.stroke,
    )
    scene.rectangle(
        x + (width - 0.24 * size) / 2.0,
        y + 0.38 * size,
        0.24 * size,
        0.24 * size,
        Tone(tone.stroke, WHITE),
        radius=False,
    )
    return _captured(scene, first, x, y, width, size)


SERVICE_ASPECT = 1.80


def service(
    scene: Scene,
    x: float,
    y: float,
    size: float,
    *,
    tone: Tone = WIRE,
    instance_tone: Tone = WORKLOAD,
) -> Mark:
    first = len(scene.elements)
    width = SERVICE_ASPECT * size
    name_width = 0.80 * size
    scene.rectangle(x, y + 0.30 * size, name_width, 0.40 * size, tone)
    instance_x = x + width - 0.45 * size
    for top in (0.0, 0.58):
        scene.rectangle(instance_x, y + top * size, 0.45 * size, 0.42 * size, instance_tone)
        scene.line(
            [
                (x + name_width, y + 0.50 * size),
                (instance_x, y + (top + 0.21) * size),
            ],
            stroke=tone.stroke,
            stroke_width=1,
        )
    return _captured(scene, first, x, y, width, size)


DNS_LOOKUP_ASPECT = 1.50


def dns_lookup(
    scene: Scene, x: float, y: float, size: float, *, tone: Tone = WIRE
) -> Mark:
    first = len(scene.elements)
    width = DNS_LOOKUP_ASPECT * size
    scene.rectangle(x, y + 0.22 * size, 1.05 * size, 0.56 * size, tone)
    for row in (0.38, 0.56):
        scene.line(
            [(x + 0.12 * size, y + row * size), (x + 0.72 * size, y + row * size)],
            stroke=tone.stroke,
            stroke_width=1,
        )
    scene.ellipse(
        x + 0.78 * size, y + 0.04 * size, 0.52 * size, 0.52 * size, OUTLINE_ONLY
    )
    scene.line(
        [(x + 1.20 * size, y + 0.50 * size), (x + 1.44 * size, y + 0.90 * size)],
        stroke=INK_SOFT,
    )
    return _captured(scene, first, x, y, width, size)


LOAD_BALANCER_ASPECT = 2.10


def load_balancer(
    scene: Scene,
    x: float,
    y: float,
    size: float,
    *,
    tone: Tone = CONTROL,
    instance_tone: Tone = WORKLOAD,
) -> Mark:
    first = len(scene.elements)
    width = LOAD_BALANCER_ASPECT * size
    scene.arrow(
        [(x, y + 0.50 * size), (x + 0.34 * size, y + 0.50 * size)], stroke=tone.stroke
    )
    scene.diamond(x + 0.38 * size, y + 0.22 * size, 0.62 * size, 0.56 * size, tone)
    for top in (0.0, 0.30, 0.60):
        centre_y = y + (top + 0.20) * size
        scene.arrow(
            [(x + 1.02 * size, y + 0.50 * size), (x + 1.58 * size, centre_y)],
            stroke=tone.stroke,
            stroke_width=1,
        )
        scene.rectangle(
            x + 1.62 * size, y + top * size, 0.40 * size, 0.40 * size, instance_tone
        )
    return _captured(scene, first, x, y, width, size)


VOLUME_ASPECT = 0.90


def volume(
    scene: Scene, x: float, y: float, size: float, *, tone: Tone = STORE
) -> Mark:
    first = len(scene.elements)
    width = VOLUME_ASPECT * size
    scene.ellipse(x, y + 0.72 * size, width, 0.28 * size, tone)
    scene.rectangle(x, y + 0.14 * size, width, 0.72 * size, tone, radius=False)
    scene.ellipse(x, y, width, 0.28 * size, Tone(tone.stroke, WHITE))
    return _captured(scene, first, x, y, width, size)


SECRET_ASPECT = 0.80


def secret(scene: Scene, x: float, y: float, size: float, *, tone: Tone = STORE) -> Mark:
    first = len(scene.elements)
    width = SECRET_ASPECT * size
    scene.ellipse(
        x + 0.18 * size, y, 0.44 * size, 0.48 * size, Tone(tone.stroke, TRANSPARENT)
    )
    scene.rectangle(x, y + 0.32 * size, width, 0.68 * size, tone)
    scene.ellipse(
        x + 0.33 * size, y + 0.56 * size, 0.14 * size, 0.14 * size, Tone(tone.stroke, tone.stroke)
    )
    return _captured(scene, first, x, y, width, size)


REPLICA_SET_ASPECT = 2.40
REPLICA_GAP_FRACTION = 0.18


# NOTE: the declared box is a fixed 2.4 x size whatever the count, so the replicas
# scale to fit it rather than the box growing. An earlier version held the replica
# size fixed and divided the leftover into gaps, which went negative at four and
# drew them overlapping.
def replica_set(
    scene: Scene,
    x: float,
    y: float,
    size: float,
    *,
    tone: Tone = WORKLOAD,
    replicas: int = 3,
) -> Mark:
    first = len(scene.elements)
    width = REPLICA_SET_ASPECT * size
    count = max(replicas, 1)
    replica_width = width / (count + REPLICA_GAP_FRACTION * (count - 1))
    gap = REPLICA_GAP_FRACTION * replica_width
    replica_size = replica_width / CONTAINER_ASPECT
    for index in range(count):
        container(
            scene,
            x + index * (replica_width + gap),
            y + size - replica_size,
            replica_size,
            tone=tone,
        )
    scene.line(
        [
            (x + 0.02 * size, y + 0.30 * size),
            (x + 0.02 * size, y + 0.10 * size),
            (x + width - 0.02 * size, y + 0.10 * size),
            (x + width - 0.02 * size, y + 0.30 * size),
        ],
        stroke=tone.stroke,
        stroke_width=1,
    )
    return _captured(scene, first, x, y, width, size)


DRAIN_ASPECT = 1.92


def drain(
    scene: Scene,
    x: float,
    y: float,
    size: float,
    *,
    node_tone: Tone = NODE,
    workload_tone: Tone = WORKLOAD,
) -> Mark:
    first = len(scene.elements)
    node_size = 0.70 * size
    node_width = HOST_ASPECT * node_size
    host(scene, x, y + 0.30 * size, node_size, tone=node_tone)
    scene.arrow(
        [
            (x + node_width + 0.04 * size, y + 0.55 * size),
            (x + node_width + 0.30 * size, y + 0.55 * size),
        ],
        stroke=RED,
    )
    moved_size = 0.44 * size
    container(
        scene,
        x + node_width + 0.36 * size,
        y + 0.30 * size,
        moved_size,
        tone=workload_tone,
    )
    return _captured(scene, first, x, y, DRAIN_ASPECT * size, size)


ROLLBACK_ASPECT = 1.15


def rollback(
    scene: Scene, x: float, y: float, size: float, *, tone: Tone = GAP
) -> Mark:
    first = len(scene.elements)
    scene.arrow(
        [
            (x + 0.98 * size, y + 0.24 * size),
            (x + 1.06 * size, y + 0.52 * size),
            (x + 0.92 * size, y + 0.80 * size),
            (x + 0.56 * size, y + 0.98 * size),
            (x + 0.20 * size, y + 0.80 * size),
            (x + 0.06 * size, y + 0.52 * size),
            (x + 0.20 * size, y + 0.24 * size),
            (x + 0.56 * size, y + 0.06 * size),
        ],
        stroke=tone.stroke,
        stroke_width=2,
    )
    return _captured(scene, first, x, y, ROLLBACK_ASPECT * size, size)


SOURCE_FILE_ASPECT = 0.78


def source_file(
    scene: Scene, x: float, y: float, size: float, *, tone: Tone = CONTROL
) -> Mark:
    first = len(scene.elements)
    width = SOURCE_FILE_ASPECT * size
    fold = 0.24 * size
    scene.rectangle(x, y, width, size, tone, radius=False)
    scene.line(
        [(x + width - fold, y), (x + width - fold, y + fold), (x + width, y + fold)],
        stroke=tone.stroke,
        stroke_width=1,
    )
    scene.line(
        [(x + width - fold, y), (x + width, y + fold)],
        stroke=tone.stroke,
        stroke_width=1,
    )
    for row, indent in ((0.44, 0.14), (0.60, 0.26), (0.76, 0.14)):
        scene.line(
            [
                (x + indent * width, y + row * size),
                (x + 0.86 * width, y + row * size),
            ],
            stroke=tone.stroke,
            stroke_width=1,
        )
    return _captured(scene, first, x, y, width, size)


OS_INSTALL_HOST_FRACTION = 0.52
OS_INSTALL_ASPECT = HOST_ASPECT * OS_INSTALL_HOST_FRACTION


def os_install(
    scene: Scene,
    x: float,
    y: float,
    size: float,
    *,
    tone: Tone = NODE,
    payload_tone: Tone | None = None,
) -> Mark:
    first = len(scene.elements)
    payload = payload_tone if payload_tone is not None else tone
    width = OS_INSTALL_ASPECT * size
    slab_height = 0.12 * size
    for row in (0.0, 0.15):
        scene.rectangle(
            x + 0.26 * width,
            y + row * size,
            0.48 * width,
            slab_height,
            payload,
            radius=False,
        )
    scene.arrow(
        [(x + width / 2.0, y + 0.31 * size), (x + width / 2.0, y + 0.45 * size)],
        stroke=payload.stroke,
        stroke_width=2,
    )
    host(scene, x, y + (1.0 - OS_INSTALL_HOST_FRACTION) * size, OS_INSTALL_HOST_FRACTION * size, tone=tone)
    return _captured(scene, first, x, y, width, size)


DISK_LAYOUT_ASPECT = 1.90
DISK_LAYOUT_PARTITIONS = (0.34, 0.18, 0.30, 0.18)


def disk_layout(
    scene: Scene, x: float, y: float, size: float, *, tone: Tone = STORE
) -> Mark:
    first = len(scene.elements)
    width = DISK_LAYOUT_ASPECT * size
    for top, shares in (
        (0.0, DISK_LAYOUT_PARTITIONS),
        (0.58, tuple(reversed(DISK_LAYOUT_PARTITIONS))),
    ):
        cursor = x
        for share in shares:
            scene.rectangle(
                cursor,
                y + top * size,
                share * width,
                0.42 * size,
                tone,
                radius=False,
                stroke_width=1,
            )
            cursor += share * width
    return _captured(scene, first, x, y, width, size)


IconDrawer = Callable[..., Mark]


class IconSpec(NamedTuple):
    name: str
    draw: IconDrawer
    aspect: float


CATALOGUE: tuple[IconSpec, ...] = (
    IconSpec("container", container, CONTAINER_ASPECT),
    IconSpec("container image", container_image, CONTAINER_IMAGE_ASPECT),
    IconSpec("registry", registry, REGISTRY_ASPECT),
    IconSpec("host", host, HOST_ASPECT),
    IconSpec("cluster", cluster, CLUSTER_ASPECT),
    IconSpec("scheduler", scheduler, SCHEDULER_ASPECT),
    IconSpec("pending workload", pending_workload, PENDING_WORKLOAD_ASPECT),
    IconSpec("binding", binding, BINDING_ASPECT),
    IconSpec("health probe", health_probe, HEALTH_PROBE_ASPECT),
    IconSpec("drift", drift, DRIFT_ASPECT),
    IconSpec("network link", network_link, NETWORK_LINK_ASPECT),
    IconSpec("service", service, SERVICE_ASPECT),
    IconSpec("DNS / SRV lookup", dns_lookup, DNS_LOOKUP_ASPECT),
    IconSpec("load balancer", load_balancer, LOAD_BALANCER_ASPECT),
    IconSpec("volume", volume, VOLUME_ASPECT),
    IconSpec("secret", secret, SECRET_ASPECT),
    IconSpec("replica set", replica_set, REPLICA_SET_ASPECT),
    IconSpec("drain", drain, DRAIN_ASPECT),
    IconSpec("rollback", rollback, ROLLBACK_ASPECT),
    IconSpec("source file", source_file, SOURCE_FILE_ASPECT),
    IconSpec("operating system install", os_install, OS_INSTALL_ASPECT),
    IconSpec("disk layout", disk_layout, DISK_LAYOUT_ASPECT),
)


def catalogue_names() -> Sequence[str]:
    return tuple(spec.name for spec in CATALOGUE)
