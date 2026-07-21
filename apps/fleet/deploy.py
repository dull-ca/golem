"""Put golemd on a guest and compile the scrolls it will apply.

golemd is built as a static musl binary — one file that runs on the Debian
guest without matching its shared libraries — using a zig-cc/lld toolchain shim.
It is installed as a root systemd service, and `.emet` sources are compiled to
binary manifests on the host before being POSTed to the guest's daemon.
"""

from __future__ import annotations

import subprocess
import tempfile
from pathlib import Path

from .config import GOLEMD_GUEST_PORT, Paths
from .state import VmRecord
from .vm import FleetError, ssh_argv


MUSL_TARGET = "x86_64-unknown-linux-musl"
GOLEMD_REMOTE_PATH = "/usr/local/bin/golemd"
GOLEMD_STATE_DIR = "/var/lib/golem"
SERVICE_REMOTE_PATH = "/etc/systemd/system/golemd.service"


def static_golemd_binary(paths: Paths) -> Path:
    return (
        paths.root
        / "target"
        / MUSL_TARGET
        / "release"
        / "golemd"
    )


def _zigcc_shim(directory: Path) -> Path:
    # cargo passes the musl/gnu `--target=` triple to the C compiler, but `zig
    # cc` names its target differently; strip those flags and hand zig its own
    # `-target x86_64-linux-musl` so the C bits link static against musl.
    shim = directory / "zigcc"
    shim.write_text(
        "#!/usr/bin/env bash\n"
        "args=()\n"
        'for a in "$@"; do\n'
        "  case \"$a\" in\n"
        "    --target=x86_64-unknown-linux-musl|--target=x86_64-unknown-linux-gnu) ;;\n"
        '    *) args+=("$a") ;;\n'
        "  esac\n"
        "done\n"
        'exec zig cc -target x86_64-linux-musl "${args[@]}"\n'
    )
    shim.chmod(0o755)
    return shim


def _zigar_shim(directory: Path) -> Path:
    shim = directory / "zigar"
    shim.write_text('#!/usr/bin/env bash\nexec zig ar "$@"\n')
    shim.chmod(0o755)
    return shim


def _rust_lld(paths: Paths) -> Path:
    import shutil

    rustc = shutil.which("rustc")
    if rustc is None:
        raise FleetError("rustc not on PATH; enter the devenv shell")
    lld = Path(rustc).resolve().parent.parent / "lib" / "rustlib" / "x86_64-unknown-linux-gnu" / "bin" / "rust-lld"
    if not lld.exists():
        raise FleetError(f"rust-lld not found at {lld}")
    return lld


def build_static_golemd(paths: Paths, force: bool = False) -> Path:
    """Build (or reuse) the static musl golemd. Points cargo's musl target at the
    zig-cc/zig-ar shims and rust-lld, so the result is one self-contained binary
    scp-able to any guest. `force` rebuilds even if the binary already exists."""
    binary = static_golemd_binary(paths)
    if binary.exists() and not force:
        return binary
    shim_dir = paths.fleet_dir / "toolchain"
    shim_dir.mkdir(parents=True, exist_ok=True)
    zigcc = _zigcc_shim(shim_dir)
    zigar = _zigar_shim(shim_dir)
    lld = _rust_lld(paths)
    import os

    env = dict(os.environ)
    env.update(
        {
            "CC_x86_64_unknown_linux_musl": str(zigcc),
            "AR_x86_64_unknown_linux_musl": str(zigar),
            "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER": str(lld),
            "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS": "-C linker-flavor=ld.lld -C link-self-contained=yes",
        }
    )
    result = subprocess.run(
        [
            "cargo",
            "build",
            "--release",
            "--target",
            MUSL_TARGET,
            "-p",
            "golemd",
        ],
        cwd=str(paths.root),
        env=env,
    )
    if result.returncode != 0:
        raise FleetError("static golemd build failed")
    if not binary.exists():
        raise FleetError(f"expected {binary} after build, not found")
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


def service_unit(host: str) -> str:
    """The golemd systemd unit for a guest: runs as root, listens on
    `0.0.0.0:7474` (reachable via the forwarded port), and drives the real
    `host` reconciler — `--reconciler host` is what makes it enact glyphs on the
    guest rather than merely bookkeep them."""
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
                    f"0.0.0.0:{GOLEMD_GUEST_PORT}",
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
    """Install the binary and unit on a guest and (re)start the service. scp to a
    staging path, `install` it into place, write the unit, then daemon-reload and
    `restart` — restart, not just `enable --now`, so a redeployed binary actually
    replaces the running one instead of leaving the old process up."""
    staging = "/tmp/golemd"
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
    _ssh_check(
        paths,
        record,
        ["sudo", "tee", SERVICE_REMOTE_PATH],
        input_text=service_unit(record.name),
    )
    _ssh_check(paths, record, ["sudo", "systemctl", "daemon-reload"])
    _ssh_check(paths, record, ["sudo", "systemctl", "enable", "golemd"])
    _ssh_check(paths, record, ["sudo", "systemctl", "restart", "golemd"])
