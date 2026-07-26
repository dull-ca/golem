"""The `fleet` CLI: boot VMs, deploy golemd, apply scrolls, and read the results.

The usual arc is `up` → `deploy` → `apply` → `logs`/`status`, and `reset` or
`down` to tear it back down. Most commands take `--hosts` to target a subset;
without it they hit every VM in the state file.
"""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
import time
from pathlib import Path
from typing import Optional

import httpx
import typer
from rich.console import Console
from rich.markup import escape
from rich.table import Table

from . import config, golemd_client, vm
from . import deploy as deploy_ops
from .config import paths, plan_hosts
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
    registry=5000:5000` exposes only the registry guest's `:5000` on host
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
            f"(ssh {plan.ssh_port}, golemd {plan.golemd_port}{extra})…"
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
    for record in records:
        console.print(f"[bold]Deploying golemd to {record.name}[/bold]…")
        deploy_ops.deploy_golemd(p, record, binary)
        summary = None
        for _ in range(30):
            summary = golemd_client.status(record)
            if summary is not None:
                break
            time.sleep(1)
        if summary is None:
            console.print(f"  [red]{record.name}: golemd did not answer /status[/red]")
        else:
            console.print(f"  [green]{record.name}: golemd up[/green] {summary}")


def _cid_hex(value: object) -> Optional[str]:
    if value is None:
        return None
    if isinstance(value, str):
        return value
    if isinstance(value, list):
        try:
            return bytes(value).hex()
        except (ValueError, TypeError):
            return str(value)
    return str(value)


def _cid_short(cid: Optional[str], width: int = 12) -> str:
    if not cid:
        return "—"
    return cid[:width] + "…" if len(cid) > width else cid


def _glyph_desc(glyph: object) -> str:
    if not isinstance(glyph, dict) or not glyph:
        return "?"
    (kind, body) = next(iter(glyph.items()))
    body = body or {}
    if kind == "AptPackage":
        return f"apt {body.get('name')}"
    if kind == "SystemdService":
        return f"systemd {body.get('unit')}"
    if kind == "LineInFile":
        return f"line {body.get('path')}: {body.get('line')}"
    if kind == "Filesystem":
        path = body.get("path")
        entry = body.get("entry")
        ekind = next(iter(entry)) if isinstance(entry, dict) and entry else "File"
        ebody = entry.get(ekind, {}) if isinstance(entry, dict) else {}
        if ekind == "Symlink":
            return f"symlink {path} → {ebody.get('target')}"
        if ekind == "Directory":
            return f"dir {path}"
        return f"file {path}"
    return str(kind)


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


_OP_VERB = {"Install": "install", "Remove": "remove", "Replace": "replace", "Noop": "noop"}


def _op_parts(op: object) -> tuple[str, str, Optional[str]]:
    if not isinstance(op, dict) or not op:
        return ("?", "?", None)
    (kind, body) = next(iter(op.items()))
    body = body or {}
    verb = _OP_VERB.get(kind, str(kind).lower())
    cid = _cid_hex(body.get("cid") if body.get("cid") is not None else body.get("new_cid"))
    return (verb, _glyph_desc(body.get("glyph")), cid)


_GLYPH_KEY_KIND = {"apt": "apt", "systemd": "systemd", "file": "file", "fileline": "line"}


def _glyph_key_desc(glyph_key: Optional[str]) -> str:
    if not glyph_key:
        return "?"
    prefix, _, rest = glyph_key.partition(":")
    kind = _GLYPH_KEY_KIND.get(prefix)
    if kind is None:
        return glyph_key
    return f"{kind} {rest}"


_UNIT_COLOR = {"settled": "green", "partial": "yellow", "rolled_back": "red"}

_GLYPH_OUTCOME_STYLE = {
    "applied": ("✓", "green", "applied"),
    "unchanged": ("·", "dim", "unchanged"),
    "rolled_back": ("↩", "yellow", "rolled back"),
    "failed": ("✗", "red", "failed"),
}


def _render_report(name: str, report: dict) -> None:
    revision = report.get("revision") or {}
    units = report.get("units") or []
    _render_revision(name, revision, units)
    top = report.get("outcome", "")
    color = _UNIT_COLOR.get(top, "white")
    console.print(f"  [{color}]apply {top}[/{color}]")
    for unit in units:
        path = " / ".join(unit.get("unit_path") or [])
        outcome = unit.get("outcome", "")
        ucolor = _UNIT_COLOR.get(outcome, "white")
        console.print(f"    [{ucolor}]{path}: {outcome}[/{ucolor}]")
        glyphs = unit.get("glyphs")
        failures = unit.get("failures") or []
        if glyphs:
            # The glyphs list already shows every failure, so its per-glyph
            # `class` and `details` (absent from a line) are looked up from
            # `failures` by key.
            classes = {f.get("glyph_key"): f.get("class", "") for f in failures}
            details = {
                f.get("glyph_key"): f.get("details")
                for f in failures
                if f.get("details")
            }
            for glyph in glyphs:
                _render_glyph_line(glyph, classes, details)
        else:
            # NOTE: a golemd predating ADR 0029's per-glyph lines omits `glyphs`;
            # `failures` is the fallback and shows only what failed.
            for failure in failures:
                _render_failure_line(failure)


def _tries(attempts: int) -> str:
    return "1 try" if attempts == 1 else f"{attempts} tries"


def _first_line(message: str) -> str:
    lines = message.splitlines()
    remaining = list(lines)
    first = ""
    while remaining:
        candidate = remaining.pop(0)
        if candidate.strip():
            first = candidate
            break
    if any(line.strip() for line in remaining):
        return f"{first}…"
    return first


_DETAILS_MAX_LINES = 50


def _render_details_block(details: Optional[str]) -> None:
    if not details:
        return
    # NOTE: forensics are raw systemctl/journalctl output — untrusted for rich
    # markup, so escape every line or a stray `[...]` would be parsed as a tag.
    # golemd already caps the payload at 50 lines; slicing here just guards a
    # future sender that doesn't.
    for line in details.splitlines()[:_DETAILS_MAX_LINES]:
        console.print(f"        [dim]{escape(line)}[/dim]")


def _render_glyph_line(glyph: dict, classes: dict, details: dict) -> None:
    desc = _glyph_key_desc(glyph.get("glyph_key"))
    outcome = glyph.get("outcome", "")
    if outcome == "failed":
        cls = classes.get(glyph.get("glyph_key")) or "failed"
        attempts = glyph.get("attempts", 0)
        message = glyph.get("message") or ""
        console.print(
            f"      [red]✗ {desc}  {cls} after {_tries(attempts)} — {_first_line(message)}[/red]"
        )
        _render_details_block(details.get(glyph.get("glyph_key")))
        return
    mark, style, label = _GLYPH_OUTCOME_STYLE.get(outcome, ("·", "white", outcome))
    console.print(f"      [{style}]{mark} {desc}  {label}[/{style}]")


def _render_failure_line(failure: dict) -> None:
    desc = _glyph_key_desc(failure.get("glyph_key"))
    cls = failure.get("class", "")
    attempts = failure.get("attempts", 0)
    message = failure.get("message", "")
    console.print(
        f"      [red]✗ {desc}  {cls} after {_tries(attempts)} — {_first_line(message)}[/red]"
    )
    _render_details_block(failure.get("details"))


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


def _render_apply_error(name: str, status: int, body: dict) -> None:
    kind = body.get("kind", "error")
    message = body.get("message", "")
    console.print(f"  [red]{name}: {kind} (HTTP {status})[/red]\n  {message}")


def _render_apply_transport_error(name: str, error: Exception) -> None:
    console.print(
        f"  [red]{name}: lost the connection waiting on the reconcile ({error}).\n"
        f"  golemd keeps reconciling server-side — it does not stop because the\n"
        f"  client gave up. Watch it with `fleet logs {name} -f`, then re-run\n"
        f"  `fleet apply` once it settles to get the report.[/red]"
    )


def _render_revision(name: str, revision: dict, units: Optional[list] = None) -> None:
    scroll = _cid_short(_cid_hex(revision.get("scroll_content_id")))
    console.print(
        f"  [green]{name}: revision {revision.get('id')}[/green] "
        f"([cyan]{revision.get('kind', '')}[/cyan])  scroll [dim]{scroll}[/dim]"
    )
    outcomes = revision.get("outcomes") or []
    if not outcomes:
        units = units or []
        if any(u.get("outcome") == "rolled_back" for u in units):
            console.print("    [yellow]nothing committed — rolled back[/yellow]")
        elif units and all(u.get("outcome") == "settled" for u in units):
            console.print("    [dim]already up to date[/dim]")
        else:
            console.print("    [dim]no changes[/dim]")
        return
    table = Table("", "op", "glyph", "content-id", box=None, pad_edge=False, show_header=False)
    for outcome in outcomes:
        verb, glyph, cid = _op_parts(outcome.get("op"))
        mark = "[green]✓[/green]" if outcome.get("changed") else "[dim]·[/dim]"
        table.add_row(mark, verb, glyph, f"[dim]{_cid_short(cid)}[/dim]")
    console.print(table)


@app.command()
def apply(
    source: Path = typer.Argument(..., help="A .emet source or a prebuilt manifest.bin."),
    hosts: Optional[str] = typer.Option(None, "--hosts", help="Comma-separated VM names."),
    raw: bool = typer.Option(False, "--json", help="Print the raw revision JSON instead of a summary."),
) -> None:
    """Compile a scroll and POST it to each target's golemd. `source` is an
    `.emet` file (compiled to a manifest here) or a prebuilt `manifest.bin`; the
    same bytes go to every host, and each prints the revision it recorded."""
    p = paths()
    state = _state()
    records = _target_records(state, hosts)
    console.print(f"[bold]Compiling {source}…[/bold]")
    manifest_path = deploy_ops.compile_manifest(p, source)
    manifest = manifest_path.read_bytes()
    console.print(f"  manifest: {manifest_path} ({len(manifest)} bytes)")
    scroll_names = deploy_ops.manifest_scroll_names(p, source)
    _render_manifest_context(scroll_names, [record.name for record in records])
    for record in records:
        console.print(f"[bold]Applying to {record.name}[/bold]…")
        try:
            response = golemd_client.apply_manifest(record, manifest)
        except (httpx.TimeoutException, httpx.TransportError) as error:
            _render_apply_transport_error(record.name, error)
            continue
        # NOTE: a partial or rolled-back reconcile is HTTP 200 with its failures
        # in-band in the report (ADR 0029 §5); non-2xx is a transport/daemon
        # error carrying a typed {kind, message} body.
        if response.status_code != 200:
            try:
                body = response.json()
            except ValueError:
                body = {"kind": "error", "message": response.text}
            _render_apply_error(record.name, response.status_code, body)
            continue
        report = response.json()
        if raw:
            console.print(f"  [green]{record.name}: revision {report.get('revision', {}).get('id')}[/green]")
            console.print_json(json.dumps(report))
        else:
            _render_report(record.name, report)


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
    for record in records:
        running = vm.is_running(record)
        summary = golemd_client.status(record) if running else None
        view = golemd_client.state(record) if summary is not None else None
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
