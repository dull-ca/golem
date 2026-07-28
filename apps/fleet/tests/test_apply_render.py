import io
import subprocess
import unittest
from pathlib import Path
from unittest import mock

from rich.console import Console
from typer.testing import CliRunner


class ManifestContextTests(unittest.TestCase):
    def test_manifest_context_lists_scrolls_and_skips(self):
        from fleet import cli
        buf = io.StringIO()
        console = Console(file=buf, force_terminal=False, no_color=True, width=200)
        with mock.patch.object(cli, "console", console):
            cli._render_manifest_context(
                ["scaly", "manta", "orbit", "talos", "kaiju", "zulip"], ["scaly"]
            )
        out = buf.getvalue()
        self.assertIn("manifest: 6 scrolls", out)
        self.assertIn("scaly, manta, orbit, talos, kaiju, zulip", out)
        self.assertIn("applying to 1 running host: scaly", out)
        self.assertIn("5 skipped: manta, orbit, talos, kaiju, zulip", out)


class ApplyExecTests(unittest.TestCase):
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

    def _invoke(self, manifest_path, records, fake_run):
        from fleet import cli

        with (
            mock.patch.object(cli, "_target_records", return_value=records),
            mock.patch.object(cli.deploy_ops, "compile_manifest", return_value=manifest_path),
            mock.patch.object(cli.deploy_ops, "manifest_scroll_names", return_value=[]),
            mock.patch.object(cli.deploy_ops, "resolve_golemctl", return_value=Path("/usr/bin/golemctl")),
            mock.patch("fleet.cli.subprocess.run", side_effect=fake_run),
        ):
            runner = CliRunner()
            return runner.invoke(cli.app, ["apply", str(manifest_path)])

    def test_apply_execs_golemctl_per_host_and_continues_on_failure(self):
        from fleet import cli

        records = self._records()
        manifest_path = self._manifest_path()

        calls = []

        def fake_run(argv, **kwargs):
            calls.append(argv)
            rc = 1 if "8001" in " ".join(argv) else 0
            return subprocess.CompletedProcess(argv, rc)

        result = self._invoke(manifest_path, records, fake_run)

        self.assertNotIn("Traceback", result.output)
        self.assertEqual(len(calls), 2)
        joined = [" ".join(str(a) for a in argv) for argv in calls]
        self.assertTrue(any("127.0.0.1:8001" in j for j in joined))
        self.assertTrue(any("127.0.0.1:8002" in j for j in joined))
        self.assertTrue(all("golemctl" in j and "apply" in j for j in joined))
        self.assertIn("vm-1", result.output)

    def test_apply_exits_nonzero_when_any_host_fails(self):
        records = self._records()
        manifest_path = self._manifest_path()

        calls = []

        def fake_run(argv, **kwargs):
            calls.append(argv)
            rc = 1 if "8001" in " ".join(argv) else 0
            return subprocess.CompletedProcess(argv, rc)

        result = self._invoke(manifest_path, records, fake_run)

        self.assertEqual(len(calls), 2)
        self.assertNotEqual(result.exit_code, 0, result.output)

    def test_apply_exits_clean_when_all_hosts_succeed(self):
        records = self._records()
        manifest_path = self._manifest_path()

        calls = []

        def fake_run(argv, **kwargs):
            calls.append(argv)
            return subprocess.CompletedProcess(argv, 0)

        result = self._invoke(manifest_path, records, fake_run)

        self.assertEqual(len(calls), 2)
        self.assertEqual(result.exit_code, 0, result.output)
