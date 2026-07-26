"""VM lifecycle: cache the base image, seed cloud-init, launch qemu, reach it by ssh.

A guest boots from a copy-on-write overlay on the shared Debian genericcloud
image, configured on first boot by a read-only cloud-init seed ISO. qemu runs
detached (`-daemonize`), forwarding one ssh port and one golemd port per VM back
to localhost; its pid lives in a pidfile so a later run can stop it.
"""

from __future__ import annotations

import re
import shutil
import signal
import socket
import subprocess
import time
import urllib.request
from pathlib import Path

from . import config
from .config import HostPlan, Paths
from .state import FleetState, VmRecord


class FleetError(RuntimeError):
    pass


def ensure_ssh_key(paths: Paths) -> None:
    """Generate the fleet's ed25519 keypair once, under `.fleet/`. Its public
    half is injected into every guest via cloud-init; the private half
    authenticates ssh and scp. Kept until `reset --purge`."""
    if paths.ssh_key.exists() and paths.ssh_pubkey.exists():
        return
    paths.fleet_dir.mkdir(parents=True, exist_ok=True)
    if paths.ssh_key.exists():
        paths.ssh_key.unlink()
    subprocess.run(
        [
            "ssh-keygen",
            "-t",
            "ed25519",
            "-N",
            "",
            "-C",
            "fleet",
            "-f",
            str(paths.ssh_key),
        ],
        check=True,
        stdout=subprocess.DEVNULL,
    )


def discover_base_image_name() -> str:
    """Scrape Debian's `trixie/latest` index for the newest concrete
    genericcloud `.qcow2`, skipping the `.json` sidecars. The filename carries a
    build date, so the lexically last match is the freshest."""
    with urllib.request.urlopen(config.BASE_IMAGE_INDEX_URL, timeout=60) as response:
        html = response.read().decode("utf-8", errors="replace")
    pattern = re.escape(config.BASE_IMAGE_PATTERN) + r"[^\"'> ]*" + re.escape(
        config.BASE_IMAGE_SUFFIX
    )
    matches = re.findall(pattern, html)
    concrete = [m for m in matches if not m.endswith(".json" + config.BASE_IMAGE_SUFFIX)]
    if not concrete:
        raise FleetError(
            f"no base image matching {config.BASE_IMAGE_PATTERN}"
            f"{config.BASE_IMAGE_SUFFIX} at {config.BASE_IMAGE_INDEX_URL}"
        )
    return sorted(set(concrete))[-1]


def ensure_base_image(paths: Paths) -> Path:
    """Return the cached base image, downloading it once if absent. The download
    is resumable: it streams to a `.part` file with `curl --continue-at -` and
    only renames to the final name on success, so an interrupted fetch resumes
    rather than restarting and a half-file is never mistaken for the image."""
    paths.images_dir.mkdir(parents=True, exist_ok=True)
    existing = sorted(paths.images_dir.glob("*" + config.BASE_IMAGE_SUFFIX))
    if existing:
        return existing[0]
    name = discover_base_image_name()
    url = config.BASE_IMAGE_INDEX_URL + name
    target = paths.images_dir / name
    partial = target.with_suffix(target.suffix + ".part")
    subprocess.run(
        [
            "curl",
            "-fL",
            "--retry",
            "10",
            "--retry-delay",
            "2",
            "--retry-all-errors",
            "--continue-at",
            "-",
            "-o",
            str(partial),
            url,
        ],
        check=True,
    )
    partial.rename(target)
    return target


def _seed_user_data(name: str, pubkey: str) -> str:
    # The cloud-init contract for a guest: set the hostname, create a passwordless
    # `golem` sudoer whose only login is the injected fleet pubkey, and disable
    # ssh password auth and root login. Key-only access is what lets the harness
    # ssh in unattended right after boot. `systemd-journal` membership lets the
    # golem user read `journalctl -u <unit>` without sudo, which the reconciler's
    # forensics and on-box debugging rely on.
    return "\n".join(
        [
            "#cloud-config",
            f"hostname: {name}",
            "manage_etc_hosts: true",
            "ssh_pwauth: false",
            "disable_root: true",
            "users:",
            f"  - name: {config.GUEST_USER}",
            "    sudo: ALL=(ALL) NOPASSWD:ALL",
            "    shell: /bin/bash",
            "    groups: [systemd-journal]",
            "    lock_passwd: true",
            "    ssh_authorized_keys:",
            f"      - {pubkey}",
            "",
        ]
    )


def _seed_meta_data(name: str) -> str:
    return "\n".join(
        [
            f"instance-id: fleet-{name}",
            f"local-hostname: {name}",
            "",
        ]
    )


def build_seed_iso(paths: Paths, vm_dir: Path, name: str) -> Path:
    """Write the cloud-init `user-data`/`meta-data` and pack them into a
    `cidata`-labelled ISO the guest reads on first boot. Prefers `cloud-localds`
    and falls back to xorriso/genisoimage/mkisofs, whichever the devenv provides."""
    pubkey = paths.ssh_pubkey.read_text().strip()
    user_data = vm_dir / "user-data"
    meta_data = vm_dir / "meta-data"
    user_data.write_text(_seed_user_data(name, pubkey))
    meta_data.write_text(_seed_meta_data(name))
    seed = vm_dir / "seed.iso"
    if shutil.which("cloud-localds"):
        subprocess.run(
            ["cloud-localds", str(seed), str(user_data), str(meta_data)],
            check=True,
        )
        return seed
    tool = shutil.which("xorriso") or shutil.which("genisoimage") or shutil.which(
        "mkisofs"
    )
    if not tool:
        raise FleetError("no ISO builder found (cloud-localds/xorriso/genisoimage)")
    if tool.endswith("xorriso"):
        subprocess.run(
            [
                tool,
                "-as",
                "genisoimage",
                "-output",
                str(seed),
                "-volid",
                "cidata",
                "-joliet",
                "-rock",
                str(user_data),
                str(meta_data),
            ],
            check=True,
        )
    else:
        subprocess.run(
            [
                tool,
                "-output",
                str(seed),
                "-volid",
                "cidata",
                "-joliet",
                "-rock",
                str(user_data),
                str(meta_data),
            ],
            check=True,
        )
    return seed


def create_overlay(vm_dir: Path, base_image: Path) -> Path:
    """A per-VM copy-on-write qcow2 backed by the shared base image. Writes land
    in the overlay, so the base stays pristine and reused across every guest, and
    tearing a VM down is just deleting its `vm-<name>/` directory."""
    disk = vm_dir / "disk.qcow2"
    subprocess.run(
        [
            "qemu-img",
            "create",
            "-f",
            "qcow2",
            "-F",
            "qcow2",
            "-b",
            str(base_image.resolve()),
            str(disk),
        ],
        check=True,
        stdout=subprocess.DEVNULL,
    )
    return disk


def _read_pid(pidfile: Path) -> int:
    # qemu writes the pidfile just after daemonizing; poll briefly for it.
    for _ in range(50):
        if pidfile.exists():
            text = pidfile.read_text().strip()
            if text:
                return int(text)
        time.sleep(0.1)
    raise FleetError(f"qemu did not write pidfile {pidfile}")


def launch_qemu(
    vm_dir: Path, plan: HostPlan, disk: Path, seed: Path
) -> tuple[int, Path, Path]:
    """Boot the guest detached and return `(pid, pidfile, console_log)`.

    Two virtio drives: the copy-on-write overlay disk, and the cloud-init seed
    ISO mounted read-only (the guest only ever reads it). User-mode networking
    forwards two host ports into the guest — ssh on `plan.ssh_port` → guest 22,
    and golemd on `plan.golemd_port` → guest 7474.

    `-display none` (not `-nographic`): under `-daemonize` there is no terminal
    to attach a console to, so `-nographic`'s serial-to-stdin wiring is wrong
    here. The serial console is instead written to `console.log` for
    boot-time debugging.
    """
    pidfile = vm_dir / "qemu.pid"
    console_log = vm_dir / "console.log"
    forwards = [
        f"hostfwd=tcp:127.0.0.1:{plan.ssh_port}-:22",
        f"hostfwd=tcp:127.0.0.1:{plan.golemd_port}-:{config.GOLEMD_GUEST_PORT}",
    ]
    for host_port, guest_port in plan.publish:
        forwards.append(f"hostfwd=tcp:127.0.0.1:{host_port}-:{guest_port}")
    hostfwd = "user,id=net0," + ",".join(forwards)
    argv = [
        "qemu-system-x86_64",
        "-name",
        plan.name,
        "-enable-kvm",
        "-cpu",
        "host",
        "-m",
        str(config.GUEST_MEMORY_MB),
        "-smp",
        str(config.GUEST_CPUS),
        "-drive",
        f"file={disk},if=virtio,format=qcow2",
        "-drive",
        f"file={seed},if=virtio,format=raw,readonly=on",
        "-netdev",
        hostfwd,
        "-device",
        "virtio-net-pci,netdev=net0",
        "-display",
        "none",
        "-serial",
        f"file:{console_log}",
        "-pidfile",
        str(pidfile),
        "-daemonize",
    ]
    subprocess.run(argv, check=True)
    pid = _read_pid(pidfile)
    return pid, pidfile, console_log


def _port_open(port: int) -> bool:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.settimeout(1.0)
        return sock.connect_ex(("127.0.0.1", port)) == 0


def ssh_argv(paths: Paths, record: VmRecord, remote: list[str]) -> list[str]:
    """The ssh command line into a guest: the fleet key, its forwarded port, and
    known-host checks disabled (each fresh VM presents a new host key on a reused
    port). Append `remote` to run a command, or leave it empty for a shell."""
    argv = [
        "ssh",
        "-i",
        str(paths.ssh_key),
        "-p",
        str(record.ssh_port),
        "-o",
        "StrictHostKeyChecking=no",
        "-o",
        "UserKnownHostsFile=/dev/null",
        "-o",
        "LogLevel=ERROR",
        "-o",
        "ConnectTimeout=5",
        f"{config.GUEST_USER}@127.0.0.1",
    ]
    return argv + remote


def ssh_ready(paths: Paths, record: VmRecord) -> bool:
    if not _port_open(record.ssh_port):
        return False
    result = subprocess.run(
        ssh_argv(paths, record, ["true"]),
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    return result.returncode == 0


def wait_for_ssh(paths: Paths, record: VmRecord) -> None:
    """Block until the guest accepts an ssh command, or raise past the timeout —
    the boot-plus-cloud-init gate before the VM is usable."""
    deadline = time.monotonic() + config.SSH_READY_TIMEOUT_S
    while time.monotonic() < deadline:
        if ssh_ready(paths, record):
            return
        time.sleep(config.SSH_POLL_INTERVAL_S)
    raise FleetError(
        f"{record.name}: ssh not reachable on port {record.ssh_port} "
        f"within {config.SSH_READY_TIMEOUT_S}s"
    )


def is_running(record: VmRecord) -> bool:
    """Whether the guest's qemu is still alive, by probing its pid with signal 0.
    Prefers the live pidfile over the recorded pid in case qemu was relaunched."""
    try:
        pidfile = Path(record.pidfile)
        if pidfile.exists():
            pid = int(pidfile.read_text().strip())
        else:
            pid = record.pid
    except (ValueError, OSError):
        pid = record.pid
    try:
        import os

        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


def kill_vm(record: VmRecord) -> None:
    """Stop the guest: SIGTERM for a clean qemu shutdown, then SIGKILL if it has
    not exited within ~5s. A missing process is already-stopped, not an error."""
    import os

    pid = record.pid
    pidfile = Path(record.pidfile)
    if pidfile.exists():
        text = pidfile.read_text().strip()
        if text:
            pid = int(text)
    if pid <= 0:
        return
    try:
        os.kill(pid, signal.SIGTERM)
    except ProcessLookupError:
        return
    # Give qemu time to exit on SIGTERM before escalating to SIGKILL.
    for _ in range(50):
        try:
            os.kill(pid, 0)
        except ProcessLookupError:
            return
        time.sleep(0.1)
    try:
        os.kill(pid, signal.SIGKILL)
    except ProcessLookupError:
        return


def _has_persisted_disk(record: VmRecord | None) -> bool:
    """Whether a known VM still has its overlay disk on disk — the test that
    tells a resumable stopped VM apart from one whose `vm-<name>/` was wiped
    (by `reset`/`--purge`) and must be created fresh."""
    return record is not None and Path(record.disk).exists()


def _merge_publish(
    existing: list[tuple[int, int]], requested: tuple[tuple[int, int], ...]
) -> list[tuple[int, int]]:
    """Union the recorded forwards with newly requested ones, keeping the
    recorded ones first and in order and appending only forwards not already
    present. This is what lets a resumed VM gain the `--publish` forwards it was
    booted without, rather than demanding a `reset`."""
    merged = [(int(h), int(g)) for h, g in existing]
    for pair in requested:
        normalized = (int(pair[0]), int(pair[1]))
        if normalized not in merged:
            merged.append(normalized)
    return merged


def resume_vm(paths: Paths, state: FleetState, record: VmRecord, plan: HostPlan) -> VmRecord:
    """Re-launch qemu for a stopped-but-present VM against its existing overlay
    disk and seed ISO, preserving its recorded ports so guest data written
    before the stop survives. Resume relaunches qemu from scratch, so
    `plan.publish` may add forwards the stopped VM lacked; `_merge_publish`
    folds the requested forwards into the recorded set."""
    vm_dir = paths.vm_dir(record.name)
    disk = Path(record.disk)
    seed = vm_dir / "seed.iso"
    if not seed.exists():
        seed = build_seed_iso(paths, vm_dir, record.name)
    publish = _merge_publish(record.publish, plan.publish)
    resume_plan = HostPlan(
        name=record.name,
        ssh_port=record.ssh_port,
        golemd_port=record.golemd_port,
        publish=tuple(publish),
    )
    pid, pidfile, console_log = launch_qemu(vm_dir, resume_plan, disk, seed)
    resumed = VmRecord(
        name=record.name,
        ssh_port=record.ssh_port,
        golemd_port=record.golemd_port,
        pid=pid,
        disk=str(disk),
        pidfile=str(pidfile),
        console_log=str(console_log),
        publish=publish,
    )
    state.put(resumed)
    wait_for_ssh(paths, resumed)
    return resumed


def bring_up(paths: Paths, state: FleetState, plan: HostPlan, base_image: Path) -> VmRecord:
    """Ensure one guest is up and return its record. Idempotent: a VM already
    running is returned as-is. A stopped VM whose overlay disk survives is
    resumed against that disk (its recorded ports kept); only a name with no
    persisted disk is created fresh."""
    existing = state.get(plan.name)
    if existing and is_running(existing):
        requested = [(int(h), int(g)) for h, g in plan.publish]
        if requested and existing.publish != requested:
            raise FleetError(
                f"{plan.name} is already running with publish {existing.publish}; "
                f"stop it (`fleet down {plan.name}`) before re-publishing {requested}"
            )
        return existing
    if _has_persisted_disk(existing):
        assert existing is not None
        return resume_vm(paths, state, existing, plan)
    vm_dir = paths.vm_dir(plan.name)
    if vm_dir.exists():
        shutil.rmtree(vm_dir)
    vm_dir.mkdir(parents=True, exist_ok=True)
    disk = create_overlay(vm_dir, base_image)
    seed = build_seed_iso(paths, vm_dir, plan.name)
    pid, pidfile, console_log = launch_qemu(vm_dir, plan, disk, seed)
    record = VmRecord(
        name=plan.name,
        ssh_port=plan.ssh_port,
        golemd_port=plan.golemd_port,
        pid=pid,
        disk=str(disk),
        pidfile=str(pidfile),
        console_log=str(console_log),
        publish=[(int(h), int(g)) for h, g in plan.publish],
    )
    state.put(record)
    wait_for_ssh(paths, record)
    return record
