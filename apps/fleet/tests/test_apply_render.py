import io
import unittest
from unittest import mock

from rich.console import Console


class RenderReportTests(unittest.TestCase):
    def _render(self, report):
        from fleet import cli
        buf = io.StringIO()
        console = Console(file=buf, force_terminal=False, no_color=True, width=200)
        with mock.patch.object(cli, "console", console):
            cli._render_report("vm-1", report)
        return buf.getvalue()

    def test_settled_unit_renders_its_path(self):
        report = {
            "revision": {"id": 3, "kind": "reconcile", "scroll_content_id": None, "outcomes": []},
            "outcome": "settled",
            "units": [{"unit_path": ["host", "base"], "outcome": "settled", "failures": []}],
        }
        out = self._render(report)
        self.assertIn("host / base", out)
        self.assertIn("settled", out)

    def test_failure_line_shows_class_attempts_and_message(self):
        report = {
            "revision": {"id": 4, "kind": "reconcile", "scroll_content_id": None, "outcomes": []},
            "outcome": "rolled_back",
            "units": [{
                "unit_path": ["host", "app"],
                "outcome": "rolled_back",
                "failures": [{
                    "glyph_key": "apt:nginx",
                    "unit_path": ["host", "app"],
                    "phase": "enact",
                    "class": "retries-exhausted",
                    "attempts": 5,
                    "message": "mirror down",
                    "rolled_back": True,
                }],
            }],
        }
        out = self._render(report)
        self.assertIn("apt nginx", out)
        self.assertIn("retries-exhausted", out)
        self.assertIn("after 5 tries", out)
        self.assertIn("mirror down", out)

    def test_motivating_scenario_shows_successes_rollbacks_and_commit_state(self):
        report = {
            "revision": {"id": 2, "kind": "reconcile", "scroll_content_id": None, "outcomes": []},
            "outcome": "rolled_back",
            "units": [{
                "unit_path": ["scaly"],
                "outcome": "rolled_back",
                "glyphs": [
                    {"glyph_key": "apt:podman", "action": "install", "outcome": "rolled_back", "attempts": 1, "message": None},
                    {"glyph_key": "file:/etc/containers/systemd/fishnet.container", "action": "install", "outcome": "rolled_back", "attempts": 1, "message": None},
                    {"glyph_key": "systemd:fishnet.service", "action": "install", "outcome": "failed", "attempts": 5, "message": "unit not found"},
                ],
                "failures": [{
                    "glyph_key": "systemd:fishnet.service",
                    "unit_path": ["scaly"],
                    "phase": "enact",
                    "class": "retries-exhausted",
                    "attempts": 5,
                    "message": "unit not found",
                    "rolled_back": True,
                }],
            }],
        }
        out = self._render(report)
        self.assertIn("nothing committed", out)
        self.assertIn("↩", out)
        self.assertIn("apt podman", out)
        self.assertIn("rolled back", out)
        self.assertIn("✗", out)
        self.assertIn("systemd fishnet.service", out)
        self.assertIn("after 5 tries", out)
        self.assertIn("unit not found", out)
        self.assertEqual(out.count("systemd fishnet.service"), 1)

    def test_all_unchanged_reports_already_up_to_date(self):
        report = {
            "revision": {"id": 3, "kind": "reconcile", "scroll_content_id": None, "outcomes": []},
            "outcome": "settled",
            "units": [{
                "unit_path": ["scaly"],
                "outcome": "settled",
                "glyphs": [
                    {"glyph_key": "apt:podman", "action": "noop", "outcome": "unchanged", "attempts": 0, "message": None},
                ],
                "failures": [],
            }],
        }
        out = self._render(report)
        self.assertIn("already up to date", out)
        self.assertNotIn("no changes", out)
        self.assertIn("unchanged", out)

    def test_report_without_glyphs_falls_back_to_failures(self):
        report = {
            "revision": {"id": 4, "kind": "reconcile", "scroll_content_id": None, "outcomes": []},
            "outcome": "rolled_back",
            "units": [{
                "unit_path": ["host", "app"],
                "outcome": "rolled_back",
                "failures": [{
                    "glyph_key": "apt:nginx",
                    "unit_path": ["host", "app"],
                    "phase": "enact",
                    "class": "retries-exhausted",
                    "attempts": 5,
                    "message": "mirror down",
                    "rolled_back": True,
                }],
            }],
        }
        out = self._render(report)
        self.assertIn("apt nginx", out)
        self.assertIn("after 5 tries", out)
        self.assertIn("mirror down", out)

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

    def test_typed_error_prints_message_not_raw_text(self):
        from fleet import cli
        buf = io.StringIO()
        console = Console(file=buf, force_terminal=False, no_color=True, width=200)
        with mock.patch.object(cli, "console", console):
            cli._render_apply_error("vm-1", 500, {"kind": "wal-unreadable", "message": "Run `fleet reset`"})
        out = buf.getvalue()
        self.assertIn("Run `fleet reset`", out)
        self.assertIn("wal-unreadable", out)
