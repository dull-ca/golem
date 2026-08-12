"""The `fleet` CLI: boot VMs, deploy golemd, apply scrolls, and read the results.

The usual arc is `up` → `deploy` → `apply` → `logs`/`status`, and `reset` or
`down` to tear it back down. Most commands take `--hosts` to target a subset;
without it they hit every VM in the state file.
"""

from __future__ import annotations

import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Optional

import typer
from rich.console import Console
from rich.table import Table

from . import config, golemd_client, vm
from . import deploy as deploy_ops
from . import inventory as inventory_ops
from . import token as token_ops
from .config import Paths, paths, plan_hosts
from .state import FleetState, VmRecord

app = typer.Typer(add_completion=False, help="Ephemeral local Debian-trixie VM fleet.")
console = Console()


def _state() -> FleetState:
    return FleetState(paths().state_file)


def _resolve_hosts(hosts: Optional[str], count: Optional[int]) -> list[str]:
    # Which hosts `up` boots: an explicit --hosts list, or the first --count
    # lichess names, or all of them. --hosts and --count are mutually exclusive.
    if hosts and count is not None:
        raise typer.BadParameter("pass either --hosts or --count, not both")
    if hosts:
        return [h.strip() for h in hosts.split(",") if h.strip()]
    if count is not None:
        if count < 1 or count > len(config.LICHESS_HOSTS):
            raise typer.BadParameter(
                f"--count must be between 1 and {len(config.LICHESS_HOSTS)}"
            )
        return config.LICHESS_HOSTS[:count]
    return list(config.LICHESS_HOSTS)


def _target_records(state: FleetState, hosts: Optional[str]) -> list[VmRecord]:
    if hosts:
        names = [h.strip() for h in hosts.split(",") if h.strip()]
    else:
        names = [record.name for record in state.all()]
    records: list[VmRecord] = []
    for name in names:
        record = state.get(name)
        if record is None:
            console.print(f"[red]unknown host {name}[/red]")
            raise typer.Exit(1)
        records.append(record)
    if not records:
        console.print("no VMs")
        raise typer.Exit(1)
    return records


@app.command()
def up(
    hosts: Optional[str] = typer.Option(None, "--hosts", help="Comma-separated VM names."),
    count: Optional[int] = typer.Option(None, "--count", help="Boot the first N lichess hosts."),
    publish: Optional[list[str]] = typer.Option(
        None,
        "--publish",
        "-p",
        help="Extra tcp forward, repeatable: NAME=HOST:GUEST for one host, or HOST:GUEST for every booted host.",
    ),
) -> None:
    """Boot VMs: ensure the base image and keypair, then bring up each host.
    Pass `--hosts` for named VMs or `--count N` for the first N lichess hosts;
    with neither, boots the full lichess set. Already-running VMs are left be.
    `--publish` forwards an extra guest port to the host: `--publish
    kaiju=5000:5000` exposes only the `kaiju` guest's `:5000` on host
    `:5000` (a bare `5000:5000` would clash across hosts sharing a host port)."""
    p = paths()
    state = _state()
    names = _resolve_hosts(hosts, count)
    publish_map = config.parse_publish(publish, names) or None
    plans = plan_hosts(names, publish_map)
    console.print("[bold]Ensuring base image…[/bold]")
    base_image = vm.ensure_base_image(p)
    console.print(f"  base image: {base_image}")
    vm.ensure_ssh_key(p)
    for plan in plans:
        extra = "".join(f", :{h}→:{g}" for h, g in plan.publish)
        console.print(
            f"[bold]Booting {plan.name}[/bold] "
            f"(ssh {plan.ssh_port}, golemd loopback-only{extra})…"
        )
        record = vm.bring_up(p, state, plan, base_image)
        console.print(f"  [green]{record.name} up[/green] pid={record.pid}")


@app.command()
def ssh(
    host: str = typer.Argument(...),
    cmd: Optional[list[str]] = typer.Argument(None),
) -> None:
    """Open an interactive shell on a guest, or run a command there if given."""
    p = paths()
    record = _state().get(host)
    if record is None:
        console.print(f"[red]unknown host {host}[/red]")
        raise typer.Exit(1)
    remote = list(cmd) if cmd else []
    result = subprocess.run(vm.ssh_argv(p, record, remote))
    raise typer.Exit(result.returncode)


@app.command()
def deploy(
    hosts: Optional[str] = typer.Option(None, "--hosts", help="Comma-separated VM names."),
    rebuild: bool = typer.Option(False, "--rebuild", help="Force a fresh static build."),
) -> None:
    """Build the static golemd once, install and (re)start it on each target
    guest, then poll `/status` to confirm the daemon came up. `--rebuild` forces
    a fresh build; `--hosts` narrows the targets."""
    p = paths()
    state = _state()
    records = _target_records(state, hosts)
    console.print("[bold]Building static golemd (musl)…[/bold]")
    binary = deploy_ops.build_static_golemd(p, force=rebuild)
    console.print(f"  binary: {binary}")
    token = token_ops.ensure_token(p)
    for record in records:
        console.print(f"[bold]Deploying golemd to {record.name}[/bold]…")
        deploy_ops.deploy_golemd(p, record, binary)
        summary = None
        for _ in range(30):
            summary = golemd_client.status(p, record, token)
            if summary is not None:
                break
            time.sleep(1)
        if summary is None:
            console.print(f"  [red]{record.name}: golemd did not answer /status[/red]")
        else:
            console.print(f"  [green]{record.name}: golemd up[/green] {summary}")


@app.command()
def inventory(
    hosts: Optional[str] = typer.Option(None, "--hosts", help="Comma-separated VM names."),
    output: Optional[Path] = typer.Option(None, "--output", help="Where to write the TOML inventory."),
) -> None:
    """Write the booted VMs as a TOML inventory and print its path —
    `.fleet/inventory.toml` unless `--output` says otherwise, `--hosts` to
    narrow the set. It is the file `golemctl fleet apply|plan|status` reads
    (ADR 0038), so the local fleet drives those verbs unchanged: pass it as
    `--inventory` or export `$GOLEMCTL_INVENTORY`. Each guest is written in ssh
    form with the fleet's token file, so golemctl reaches the loopback-bound
    daemons the same way it reaches production ones (ADR 0042)."""
    p = paths()
    state = _state()
    records = _target_records(state, hosts)
    token_ops.ensure_token(p)
    dest = output if output is not None else p.inventory_file
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_text(inventory_ops.render_hosts_toml(inventory_ops.inventory_entries(p, records)))
    console.print(str(dest))


def _count_glyphs(scroll: object) -> int:
    if not isinstance(scroll, dict):
        return 0
    contents = scroll.get("contents")
    if not isinstance(contents, dict):
        return 0
    if "Glyphs" in contents:
        glyphs = contents.get("Glyphs")
        return len(glyphs) if isinstance(glyphs, list) else 0
    if "Groups" in contents:
        groups = contents.get("Groups")
        if isinstance(groups, list):
            return sum(_count_glyphs(child) for child in groups)
    return 0


def _render_manifest_context(scroll_names: list[str], applying: list[str]) -> None:
    if not scroll_names:
        return
    console.print(
        f"  manifest: {len(scroll_names)} "
        f"scroll{'s' if len(scroll_names) != 1 else ''} "
        f"([cyan]{', '.join(scroll_names)}[/cyan])"
    )
    targeted = [n for n in applying if n in scroll_names]
    skipped = [n for n in scroll_names if n not in applying]
    host_word = "host" if len(targeted) == 1 else "hosts"
    line = (
        f"  applying to {len(targeted)} running {host_word}: "
        f"[green]{', '.join(targeted) or '—'}[/green]"
    )
    if skipped:
        line += f"  [dim]({len(skipped)} skipped: {', '.join(skipped)})[/dim]"
    console.print(line)


def _ssh_inventory_file(p: Paths, records: list[VmRecord]) -> Path:
    # The inventory handed to one `golemctl fleet` run, written fresh each time
    # rather than reusing `.fleet/inventory.toml`: that file is the operator's,
    # written when they ask for it, and may name a different set of guests than
    # this invocation targets.
    token_ops.ensure_token(p)
    tmp_dir = Path(tempfile.mkdtemp(prefix="fleet-inventory-"))
    dest = tmp_dir / "inventory.toml"
    dest.write_text(inventory_ops.render_hosts_toml(inventory_ops.inventory_entries(p, records)))
    return dest


@app.command()
def apply(
    source: Path = typer.Argument(..., help="A .emet source or a prebuilt manifest.bin."),
    hosts: Optional[str] = typer.Option(None, "--hosts", help="Comma-separated VM names."),
    raw: bool = typer.Option(False, "--json", help="Print golemctl's {\"hosts\": {…}} aggregate instead of a summary."),
) -> None:
    """Compile a scroll and hand every target to one `golemctl fleet apply`.
    `source` is an `.emet` file (compiled here) or a prebuilt `manifest.bin`;
    the same bytes go to every host.

    fleet owns orchestration (which hosts); golemctl owns the apply itself — and
    since ADR 0042 that is a single fan-out call over a rendered ssh inventory,
    not a `golemctl apply` per host: golemctl opens each guest's forward, holds
    the hosts concurrently, and draws one live tree across them. fleet execs it
    with inherited stdio, because the TUI must own the terminal to draw its
    frames. The exit code is golemctl's, which is already 0 only if every host
    settled or was skipped."""
    p = paths()
    state = _state()
    records = _target_records(state, hosts)
    console.print(f"[bold]Compiling {source}…[/bold]")
    manifest_path = deploy_ops.compile_manifest(p, source)
    manifest = manifest_path.read_bytes()
    console.print(f"  manifest: {manifest_path} ({len(manifest)} bytes)")
    scroll_names = deploy_ops.manifest_scroll_names(p, source)
    _render_manifest_context(scroll_names, [record.name for record in records])
    golemctl = deploy_ops.resolve_golemctl(p)
    inventory_file = _ssh_inventory_file(p, records)
    console.print(
        f"[bold]Applying to {len(records)} "
        f"host{'s' if len(records) != 1 else ''}[/bold]…"
    )
    argv = [
        str(golemctl),
        "fleet",
        "apply",
        str(manifest_path),
        "--inventory",
        str(inventory_file),
        "--hosts",
        ",".join(record.name for record in records),
    ]
    if raw:
        argv.append("--json")
    result = subprocess.run(argv, cwd=str(p.root))
    if result.returncode != 0:
        console.print(f"  [red]golemctl fleet apply exited {result.returncode}[/red]")
        raise typer.Exit(result.returncode)


@app.command()
def plan(
    source: Path = typer.Argument(..., help="A .emet source or a prebuilt manifest.bin."),
    hosts: Optional[str] = typer.Option(None, "--hosts", help="Comma-separated VM names."),
    raw: bool = typer.Option(False, "--json", help="Print golemctl's {\"hosts\": {…}} aggregate instead of the collapsed view."),
    detail: bool = typer.Option(False, "--detail", help="One glyph per line with content ids."),
    against_host: bool = typer.Option(False, "--against-host", help="Also diff against what is actually on the host, read live."),
) -> None:
    """Compile a scroll and hand every target to one `golemctl fleet plan` — the
    dry-run diff, nothing applied (ADR 0036). Same split as `apply`: fleet picks
    the hosts and renders the ssh inventory, golemctl opens the forwards, POSTs,
    and renders each host's diff under its own heading. A plan is not a failure,
    so golemctl exits nonzero only when a host errored."""
    p = paths()
    state = _state()
    records = _target_records(state, hosts)
    console.print(f"[bold]Compiling {source}…[/bold]")
    manifest_path = deploy_ops.compile_manifest(p, source)
    manifest = manifest_path.read_bytes()
    console.print(f"  manifest: {manifest_path} ({len(manifest)} bytes)")
    scroll_names = deploy_ops.manifest_scroll_names(p, source)
    _render_manifest_context(scroll_names, [record.name for record in records])
    golemctl = deploy_ops.resolve_golemctl(p)
    inventory_file = _ssh_inventory_file(p, records)
    argv = [
        str(golemctl),
        "fleet",
        "plan",
        str(manifest_path),
        "--inventory",
        str(inventory_file),
        "--hosts",
        ",".join(record.name for record in records),
    ]
    if raw:
        argv.append("--json")
    if detail:
        argv.append("--detail")
    if against_host:
        argv.append("--against-host")
    result = subprocess.run(argv, cwd=str(p.root))
    if result.returncode != 0:
        console.print(f"  [red]golemctl fleet plan exited {result.returncode}[/red]")
        raise typer.Exit(result.returncode)


@app.command()
def logs(
    host: str = typer.Argument(...),
    follow: bool = typer.Option(False, "--follow", "-f", help="Stream new log lines."),
) -> None:
    """Tail golemd's journal on one or more comma-separated hosts. A single host
    streams inline; several are read in parallel with each line name-tagged.
    `--follow` keeps streaming new lines."""
    p = paths()
    state = _state()
    names = [h.strip() for h in host.split(",") if h.strip()]
    records = [state.get(name) for name in names]
    missing = [name for name, record in zip(names, records) if record is None]
    if missing:
        console.print(f"[red]unknown host(s) {', '.join(missing)}[/red]")
        raise typer.Exit(1)
    resolved = [record for record in records if record is not None]
    remote = ["sudo", "journalctl", "-u", "golemd", "-b", "--no-pager"]
    if follow:
        remote.append("-f")
    if len(resolved) == 1:
        result = subprocess.run(vm.ssh_argv(p, resolved[0], remote))
        raise typer.Exit(result.returncode)
    procs = []
    for record in resolved:
        proc = subprocess.Popen(
            vm.ssh_argv(p, record, remote),
            stdout=subprocess.PIPE,
            text=True,
        )
        procs.append((record.name, proc))
    try:
        for name, proc in procs:
            assert proc.stdout is not None
            for line in proc.stdout:
                console.print(f"[dim]{name}[/dim] {line.rstrip()}")
    finally:
        for _, proc in procs:
            proc.terminate()


@app.command()
def status() -> None:
    """A table across every known VM: whether qemu is up, whether golemd answers,
    and its current content-id, glyph count, and last revision."""
    p = paths()
    state = _state()
    records = state.all()
    if not records:
        console.print("no VMs")
        return
    table = Table(
        "name",
        "up",
        "golemd",
        "content-id",
        "glyphs",
        "last revision",
    )
    token = token_ops.ensure_token(p)
    for record in records:
        running = vm.is_running(record)
        summary = golemd_client.status(p, record, token) if running else None
        view = golemd_client.state(p, record, token) if summary is not None else None
        content_id = "—"
        glyph_count = "—"
        if view is not None:
            content_id = (view.get("content_id") or "—")
            if isinstance(content_id, str) and len(content_id) > 16:
                content_id = content_id[:16] + "…"
            scroll = view.get("scroll")
            glyph_count = str(_count_glyphs(scroll)) if scroll is not None else "—"
        last_revision = "—"
        if summary is not None and summary.get("latest_revision") is not None:
            last_revision = f"#{summary['latest_revision']}"
        table.add_row(
            record.name,
            "[green]up[/green]" if running else "[red]down[/red]",
            "[green]reachable[/green]" if summary is not None else "[red]—[/red]",
            str(content_id),
            glyph_count,
            last_revision,
        )
    console.print(table)


@app.command()
def down(
    host: Optional[str] = typer.Argument(None),
    all_: bool = typer.Option(False, "--all", help="Stop every VM."),
) -> None:
    """Stop a named VM (or `--all`), leaving its overlay disk and state on disk —
    a stopped VM can be brought back up. To reclaim the disks too, use `reset`."""
    state = _state()
    if all_:
        targets = state.all()
    elif host:
        record = state.get(host)
        if record is None:
            console.print(f"[red]unknown host {host}[/red]")
            raise typer.Exit(1)
        targets = [record]
    else:
        raise typer.BadParameter("pass a host name or --all")
    for record in targets:
        vm.kill_vm(record)
        console.print(f"[yellow]{record.name} stopped[/yellow]")


@app.command()
def reset(
    purge: bool = typer.Option(False, "--purge", help="Also remove cached image + keypair."),
) -> None:
    """Kill every VM and delete all per-VM data — overlay disks, state file, the
    lot — back to a clean slate; the cached image and keypair survive for a fast
    next `up`. `--purge` drops those too, so the next boot re-downloads."""
    p = paths()
    state = _state()
    for record in state.all():
        vm.kill_vm(record)
    state.clear()
    for vm_dir in p.fleet_dir.glob("vm-*"):
        if vm_dir.is_dir():
            shutil.rmtree(vm_dir)
    if p.state_file.exists():
        p.state_file.unlink()
    console.print("[green]reset: all VMs killed, per-VM dirs removed[/green]")
    if purge:
        if p.images_dir.exists():
            shutil.rmtree(p.images_dir)
        for key in (p.ssh_key, p.ssh_pubkey):
            if key.exists():
                key.unlink()
        console.print("[green]purge: cached image + keypair removed[/green]")


def main() -> None:
    try:
        app()
    except vm.FleetError as error:
        console.print(f"[red]error:[/red] {error}")
        sys.exit(1)


if __name__ == "__main__":
    main()
