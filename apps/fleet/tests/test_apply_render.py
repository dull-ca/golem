import io
import subprocess
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest import mock

from rich.console import Console
from typer.testing import CliRunner

from fleet.config import Paths


class ManifestContextTests(unittest.TestCase):
    def test_manifest_context_lists_scrolls_and_skips(self):
        from fleet import cli
        buf = io.StringIO()
        console = Console(file=buf, force_terminal=False, no_color=True, width=200)
        with mock.patch.object(cli, "console", console):
            cli._render_manifest_context(
                ["scaly", "manta", "orbit", "talos", "kaiju", "remora"], ["scaly"]
            )
        out = buf.getvalue()
        self.assertIn("manifest: 6 scrolls", out)
        self.assertIn("scaly, manta, orbit, talos, kaiju, remora", out)
        self.assertIn("applying to 1 running host: scaly", out)
        self.assertIn("5 skipped: manta, orbit, talos, kaiju, remora", out)


class ApplyExecTests(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = TemporaryDirectory()
        self.fleet_paths = Paths(root=Path(self._tmp.name))
        self.addCleanup(self._tmp.cleanup)

    def _records(self):
        from fleet.state import VmRecord

        return [
            VmRecord(name="vm-1", ssh_port=2201, golemd_port=8001, pid=1,
                     disk="/dev/null", pidfile="/dev/null", console_log="/dev/null"),
            VmRecord(name="vm-2", ssh_port=2202, golemd_port=8002, pid=2,
                     disk="/dev/null", pidfile="/dev/null", console_log="/dev/null"),
        ]

    def _manifest_path(self):
        manifest_path = Path("/tmp/fleet-test-manifest.bin")
        manifest_path.write_bytes(b"\x00")
        self.addCleanup(manifest_path.unlink, missing_ok=True)
        return manifest_path

    def _invoke(self, manifest_path, records, fake_run, args=None):
        from fleet import cli

        with (
            mock.patch.object(cli, "paths", return_value=self.fleet_paths),
            mock.patch.object(cli, "_target_records", return_value=records),
            mock.patch.object(cli.deploy_ops, "compile_manifest", return_value=manifest_path),
            mock.patch.object(cli.deploy_ops, "manifest_scroll_names", return_value=[]),
            mock.patch.object(cli.deploy_ops, "resolve_golemctl", return_value=Path("/usr/bin/golemctl")),
            mock.patch("fleet.cli.subprocess.run", side_effect=fake_run),
        ):
            runner = CliRunner()
            return runner.invoke(cli.app, ["apply", str(manifest_path), *(args or [])])

    def test_apply_execs_golemctl_fleet_apply_once_naming_every_host(self):
        from fleet import cli

        records = self._records()
        manifest_path = self._manifest_path()

        calls = []

        def fake_run(argv, **kwargs):
            calls.append(argv)
            return subprocess.CompletedProcess(argv, 0)

        result = self._invoke(manifest_path, records, fake_run)

        self.assertNotIn("Traceback", result.output)
        self.assertEqual(len(calls), 1)
        argv = [str(a) for a in calls[0]]
        self.assertIn("golemctl", argv[0])
        self.assertEqual(argv[1:3], ["fleet", "apply"])
        self.assertIn(str(manifest_path), argv)
        self.assertIn("--inventory", argv)
        inventory_path = Path(argv[argv.index("--inventory") + 1])
        self.assertTrue(inventory_path.exists())
        self.assertIn("vm-1", inventory_path.read_text())
        self.assertIn("vm-2", inventory_path.read_text())
        self.assertIn("--hosts", argv)
        self.assertEqual(argv[argv.index("--hosts") + 1], "vm-1,vm-2")
        self.assertEqual(result.exit_code, 0, result.output)

    def test_apply_exits_with_golemctls_own_exit_code(self):
        records = self._records()
        manifest_path = self._manifest_path()

        def fake_run(argv, **kwargs):
            return subprocess.CompletedProcess(argv, 1)

        result = self._invoke(manifest_path, records, fake_run)

        self.assertEqual(result.exit_code, 1, result.output)

    def test_apply_exits_clean_when_golemctl_succeeds(self):
        records = self._records()
        manifest_path = self._manifest_path()

        def fake_run(argv, **kwargs):
            return subprocess.CompletedProcess(argv, 0)

        result = self._invoke(manifest_path, records, fake_run)

        self.assertEqual(result.exit_code, 0, result.output)

    def test_apply_json_flag_is_forwarded(self):
        records = self._records()
        manifest_path = self._manifest_path()
        calls = []

        def fake_run(argv, **kwargs):
            calls.append(argv)
            return subprocess.CompletedProcess(argv, 0)

        self._invoke(manifest_path, records, fake_run, args=["--json"])

        self.assertIn("--json", [str(a) for a in calls[0]])


class PlanExecTests(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = TemporaryDirectory()
        self.fleet_paths = Paths(root=Path(self._tmp.name))
        self.addCleanup(self._tmp.cleanup)

    def _records(self):
        from fleet.state import VmRecord

        return [
            VmRecord(name="vm-1", ssh_port=2201, golemd_port=8001, pid=1,
                     disk="/dev/null", pidfile="/dev/null", console_log="/dev/null"),
            VmRecord(name="vm-2", ssh_port=2202, golemd_port=8002, pid=2,
                     disk="/dev/null", pidfile="/dev/null", console_log="/dev/null"),
        ]

    def _manifest_path(self):
        manifest_path = Path("/tmp/fleet-test-plan-manifest.bin")
        manifest_path.write_bytes(b"\x00")
        self.addCleanup(manifest_path.unlink, missing_ok=True)
        return manifest_path

    def _invoke(self, manifest_path, records, fake_run, args=None):
        from fleet import cli

        with (
            mock.patch.object(cli, "paths", return_value=self.fleet_paths),
            mock.patch.object(cli, "_target_records", return_value=records),
            mock.patch.object(cli.deploy_ops, "compile_manifest", return_value=manifest_path),
            mock.patch.object(cli.deploy_ops, "manifest_scroll_names", return_value=[]),
            mock.patch.object(cli.deploy_ops, "resolve_golemctl", return_value=Path("/usr/bin/golemctl")),
            mock.patch("fleet.cli.subprocess.run", side_effect=fake_run),
        ):
            runner = CliRunner()
            return runner.invoke(cli.app, ["plan", str(manifest_path), *(args or [])])

    def test_plan_execs_golemctl_fleet_plan_once_naming_every_host(self):
        records = self._records()
        manifest_path = self._manifest_path()

        calls = []

        def fake_run(argv, **kwargs):
            calls.append(argv)
            return subprocess.CompletedProcess(argv, 0)

        result = self._invoke(manifest_path, records, fake_run)

        self.assertEqual(len(calls), 1)
        argv = [str(a) for a in calls[0]]
        self.assertEqual(argv[1:3], ["fleet", "plan"])
        self.assertIn("--inventory", argv)
        self.assertIn("--hosts", argv)
        self.assertEqual(argv[argv.index("--hosts") + 1], "vm-1,vm-2")
        self.assertEqual(result.exit_code, 0, result.output)

    def test_plan_detail_flag_is_forwarded(self):
        records = self._records()
        manifest_path = self._manifest_path()
        calls = []

        def fake_run(argv, **kwargs):
            calls.append(argv)
            return subprocess.CompletedProcess(argv, 0)

        self._invoke(manifest_path, records, fake_run, args=["--detail"])

        self.assertIn("--detail", [str(a) for a in calls[0]])

    def test_plan_exits_with_golemctls_own_exit_code(self):
        records = self._records()
        manifest_path = self._manifest_path()

        def fake_run(argv, **kwargs):
            return subprocess.CompletedProcess(argv, 1)

        result = self._invoke(manifest_path, records, fake_run)

        self.assertEqual(result.exit_code, 1, result.output)
