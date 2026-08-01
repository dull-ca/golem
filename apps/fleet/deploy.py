"""Put golemd on a guest and compile the scrolls it will apply.

golemd is built as a static musl binary — one file that runs on the Debian
guest without matching its shared libraries — via the `golemd-static` flake
output (`nix build .#golemd-static`, pkgsStatic). It is installed as a root
systemd service, and `.emet` sources are compiled to binary manifests on the
host before being POSTed to the guest's daemon.
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import uuid
import tempfile
from pathlib import Path

from .config import GOLEMD_GUEST_PORT, Paths
from .state import VmRecord
from .token import ensure_token
from .vm import FleetError, ssh_argv


GOLEMD_STATIC_FLAKE_OUTPUT = ".#golemd-static"
GOLEMD_REMOTE_PATH = "/usr/local/bin/golemd"
GOLEMD_STATE_DIR = "/var/lib/golem"
SERVICE_REMOTE_PATH = "/etc/systemd/system/golemd.service"
GOLEMD_CONFIG_DIR = "/etc/golem"
CONFIG_REMOTE_PATH = "/etc/golem/golemd.toml"
TOKEN_REMOTE_PATH = "/etc/golem/token"


def build_static_golemd(paths: Paths, force: bool = False) -> Path:
    """Build the static musl golemd through the `golemd-static` flake output and
    return the path to the resulting binary. pkgsStatic links everything (musl
    libc, bundled sqlite, rustls crypto) into one self-contained file scp-able to
    any guest. `force` bypasses the nix build cache with `--rebuild`."""
    out_link = paths.fleet_dir / "result-golemd-static"
    out_link.parent.mkdir(parents=True, exist_ok=True)
    argv = [
        "nix",
        "build",
        GOLEMD_STATIC_FLAKE_OUTPUT,
        "--out-link",
        str(out_link),
    ]
    if force:
        argv.append("--rebuild")
    result = subprocess.run(argv, cwd=str(paths.root))
    if result.returncode != 0:
        raise FleetError("static golemd build failed (nix build .#golemd-static)")
    binary = out_link / "bin" / "golemd"
    if not binary.exists():
        raise FleetError(f"expected {binary} after nix build, not found")
    return binary


def compile_manifest(paths: Paths, source: Path) -> Path:
    """Compile an `.emet` source to a binary manifest, or pass a prebuilt
    manifest straight through. A relative source anchors at the repo root, so
    `apply examples/lichess/fleet.emet` resolves the same from any cwd."""
    if not source.is_absolute():
        source = paths.root / source
    source = source.resolve()
    if source.suffix != ".emet":
        return source
    out = Path(tempfile.mkdtemp(prefix="fleet-manifest-")) / "manifest.bin"
    result = subprocess.run(
        [
            "cargo",
            "run",
            "-q",
            "-p",
            "emet",
            "--",
            "build",
            str(source),
            "-o",
            str(out),
        ],
        cwd=str(paths.root),
    )
    if result.returncode != 0:
        raise FleetError(f"emet build of {source} failed")
    if not out.exists():
        raise FleetError(f"emet did not emit {out}")
    return out


def manifest_scroll_names(paths: Paths, source: Path) -> list[str]:
    """The scroll (host) names an `.emet` source compiles to, read from a
    `--json` build. A prebuilt manifest passed straight through has no source to
    introspect, so this returns `[]` and the caller renders no manifest context.
    A build failure is non-fatal here: names are context, not the apply itself."""
    if not source.is_absolute():
        source = paths.root / source
    source = source.resolve()
    if source.suffix != ".emet":
        return []
    result = subprocess.run(
        ["cargo", "run", "-q", "-p", "emet", "--", "build", str(source), "--json"],
        cwd=str(paths.root),
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        return []
    try:
        manifest = json.loads(result.stdout)
    except (ValueError, TypeError):
        return []
    scrolls = manifest.get("scrolls") if isinstance(manifest, dict) else None
    names: list[str] = []
    for addressed in scrolls or []:
        scroll = addressed.get("scroll") if isinstance(addressed, dict) else None
        name = scroll.get("name") if isinstance(scroll, dict) else None
        if isinstance(name, str):
            names.append(name)
    return names


def resolve_golemctl(paths: Paths) -> Path:
    """Locate the golemctl binary: `GOLEMCTL_BIN` env override, then the
    workspace's `cargo build` output, then `PATH`. There is no HTTP fallback —
    golemctl is the only apply path."""
    override = os.environ.get("GOLEMCTL_BIN")
    if override:
        return Path(override)
    built = paths.root / "target" / "debug" / "golemctl"
    if built.exists():
        return built
    found = shutil.which("golemctl")
    if found:
        return Path(found)
    raise FleetError(
        "golemctl not found; run `build` (cargo build --workspace) or set GOLEMCTL_BIN"
    )


def golemd_config_toml() -> str:
    return "\n".join(
        [
            "[auth]",
            f'token_file = "{TOKEN_REMOTE_PATH}"',
            "",
        ]
    )


def service_unit(host: str) -> str:
    return "\n".join(
        [
            "[Unit]",
            "Description=Golem bookkeeping agent",
            "After=network-online.target",
            "Wants=network-online.target",
            "",
            "[Service]",
            "Type=simple",
            "User=root",
            "ExecStart=" + " ".join(
                [
                    GOLEMD_REMOTE_PATH,
                    "--host",
                    host,
                    "--state-dir",
                    GOLEMD_STATE_DIR,
                    "--listen",
                    f"127.0.0.1:{GOLEMD_GUEST_PORT}",
                    "--config",
                    CONFIG_REMOTE_PATH,
                    "--reconciler",
                    "host",
                ]
            ),
            "Restart=on-failure",
            "RestartSec=5s",
            "",
            "[Install]",
            "WantedBy=multi-user.target",
            "",
        ]
    )


def _scp_argv(paths: Paths, record: VmRecord, local: Path, remote: str) -> list[str]:
    return [
        "scp",
        "-i",
        str(paths.ssh_key),
        "-P",
        str(record.ssh_port),
        "-o",
        "StrictHostKeyChecking=no",
        "-o",
        "UserKnownHostsFile=/dev/null",
        "-o",
        "LogLevel=ERROR",
        "-o",
        "ConnectTimeout=5",
        str(local),
        remote,
    ]


def _ssh_check(paths: Paths, record: VmRecord, remote: list[str], input_text: str | None = None) -> str:
    result = subprocess.run(
        ssh_argv(paths, record, remote),
        input=input_text,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise FleetError(
            f"{record.name}: {' '.join(remote)} failed: {result.stderr.strip() or result.stdout.strip()}"
        )
    return result.stdout


def deploy_golemd(paths: Paths, record: VmRecord, binary: Path) -> None:
    token = ensure_token(paths)
    staging = f"/home/golem/golemd.staging-{uuid.uuid4().hex[:8]}"
    scp = subprocess.run(_scp_argv(paths, record, binary, f"golem@127.0.0.1:{staging}"), capture_output=True, text=True)
    if scp.returncode != 0:
        raise FleetError(f"{record.name}: scp golemd failed: {scp.stderr.strip()}")
    _ssh_check(
        paths,
        record,
        [
            "sudo",
            "install",
            "-m",
            "0755",
            staging,
            GOLEMD_REMOTE_PATH,
        ],
    )
    _ssh_check(paths, record, ["rm", "-f", staging])
    _ssh_check(paths, record, ["sudo", "install", "-d", "-m", "0755", GOLEMD_CONFIG_DIR])
    _ssh_check(paths, record, ["sudo", "tee", TOKEN_REMOTE_PATH], input_text=token)
    _ssh_check(paths, record, ["sudo", "chown", "root:root", TOKEN_REMOTE_PATH])
    _ssh_check(paths, record, ["sudo", "chmod", "0600", TOKEN_REMOTE_PATH])
    _ssh_check(
        paths,
        record,
        ["sudo", "tee", CONFIG_REMOTE_PATH],
        input_text=golemd_config_toml(),
    )
    _ssh_check(
        paths,
        record,
        ["sudo", "tee", SERVICE_REMOTE_PATH],
        input_text=service_unit(record.name),
    )
    _ssh_check(paths, record, ["sudo", "systemctl", "daemon-reload"])
    _ssh_check(paths, record, ["sudo", "systemctl", "enable", "golemd"])
    _ssh_check(paths, record, ["sudo", "systemctl", "restart", "golemd"])
