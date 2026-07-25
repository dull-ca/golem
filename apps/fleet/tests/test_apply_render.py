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

    def test_typed_error_prints_message_not_raw_text(self):
        from fleet import cli
        buf = io.StringIO()
        console = Console(file=buf, force_terminal=False, no_color=True, width=200)
        with mock.patch.object(cli, "console", console):
            cli._render_apply_error("vm-1", 500, {"kind": "wal-unreadable", "message": "Run `fleet reset`"})
        out = buf.getvalue()
        self.assertIn("Run `fleet reset`", out)
        self.assertIn("wal-unreadable", out)
