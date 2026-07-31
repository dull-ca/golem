"""The rendered file is golemctl's contract, not the harness's: what these
assert about keys, quoting, and order is what `golemctl fleet` must be able to
parse back (ADR 0038).
"""

import unittest
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest import mock

from typer.testing import CliRunner

from fleet import inventory
from fleet.config import Paths
from fleet.state import FleetState, VmRecord


def _record(name: str, golemd_port: int) -> VmRecord:
    return VmRecord(
        name=name,
        ssh_port=2200,
        golemd_port=golemd_port,
        pid=1,
        disk="/dev/null",
        pidfile="/dev/null",
        console_log="/dev/null",
    )


class GolemdUrlTests(unittest.TestCase):
    def test_builds_a_loopback_url_for_the_forwarded_port(self) -> None:
        self.assertEqual(inventory.golemd_url(8807), "http://127.0.0.1:8807")


class InventoryEntriesTests(unittest.TestCase):
    def test_maps_each_record_to_its_name_and_golemd_url(self) -> None:
        records = [_record("scaly", 8807), _record("manta", 8842)]
        self.assertEqual(
            inventory.inventory_entries(records),
            [("scaly", "http://127.0.0.1:8807"), ("manta", "http://127.0.0.1:8842")],
        )

    def test_preserves_the_given_record_order(self) -> None:
        records = [_record("zulip", 8801), _record("kaiju", 8802)]
        entries = inventory.inventory_entries(records)
        self.assertEqual([name for name, _ in entries], ["zulip", "kaiju"])


class RenderHostsTomlTests(unittest.TestCase):
    def test_renders_the_hosts_table_header(self) -> None:
        toml = inventory.render_hosts_toml([("scaly", "http://127.0.0.1:8807")])
        self.assertEqual(toml.splitlines()[0], "[hosts]")

    def test_renders_one_line_per_entry(self) -> None:
        toml = inventory.render_hosts_toml(
            [("scaly", "http://127.0.0.1:8807"), ("manta", "http://127.0.0.1:8842")]
        )
        self.assertEqual(
            toml,
            "[hosts]\n"
            'scaly = "http://127.0.0.1:8807"\n'
            'manta = "http://127.0.0.1:8842"\n',
        )

    def test_preserves_input_order_rather_than_sorting(self) -> None:
        toml = inventory.render_hosts_toml(
            [("zulip", "http://127.0.0.1:8801"), ("kaiju", "http://127.0.0.1:8802")]
        )
        lines = toml.splitlines()
        self.assertEqual(lines[1], 'zulip = "http://127.0.0.1:8801"')
        self.assertEqual(lines[2], 'kaiju = "http://127.0.0.1:8802"')

    def test_empty_entries_renders_just_the_header(self) -> None:
        self.assertEqual(inventory.render_hosts_toml([]), "[hosts]\n")

    def test_quotes_a_key_that_is_not_a_bare_toml_key(self) -> None:
        toml = inventory.render_hosts_toml([("has space", "http://127.0.0.1:8807")])
        self.assertIn('"has space" = "http://127.0.0.1:8807"', toml)

    def test_escapes_backslashes_and_quotes_in_the_value(self) -> None:
        toml = inventory.render_hosts_toml([("scaly", 'http://127.0.0.1:8807/"x"\\y')])
        self.assertIn('"http://127.0.0.1:8807/\\"x\\"\\\\y"', toml)


class InventoryCommandTests(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = TemporaryDirectory()
        self.root = Path(self._tmp.name)
        self.addCleanup(self._tmp.cleanup)

    def _write_state(self, records: list[VmRecord]) -> Paths:
        fleet_paths = Paths(root=self.root)
        state = FleetState(fleet_paths.state_file)
        for record in records:
            state.put(record)
        return fleet_paths

    def _invoke(self, fleet_paths: Paths, args: list[str]):
        from fleet import cli

        with mock.patch.object(cli, "paths", return_value=fleet_paths):
            runner = CliRunner()
            return runner.invoke(cli.app, ["inventory", *args])

    def test_writes_the_default_inventory_file_and_prints_its_path(self) -> None:
        fleet_paths = self._write_state([_record("scaly", 8807), _record("manta", 8842)])

        result = self._invoke(fleet_paths, [])

        self.assertEqual(result.exit_code, 0, result.output)
        dest = fleet_paths.inventory_file
        self.assertIn(str(dest), result.output)
        self.assertEqual(
            dest.read_text(),
            '[hosts]\nscaly = "http://127.0.0.1:8807"\nmanta = "http://127.0.0.1:8842"\n',
        )

    def test_hosts_option_filters_to_the_named_subset(self) -> None:
        fleet_paths = self._write_state([_record("scaly", 8807), _record("manta", 8842)])

        result = self._invoke(fleet_paths, ["--hosts", "manta"])

        self.assertEqual(result.exit_code, 0, result.output)
        self.assertEqual(
            fleet_paths.inventory_file.read_text(),
            '[hosts]\nmanta = "http://127.0.0.1:8842"\n',
        )

    def test_unknown_host_name_errors_clearly(self) -> None:
        fleet_paths = self._write_state([_record("scaly", 8807)])

        result = self._invoke(fleet_paths, ["--hosts", "nope"])

        self.assertNotEqual(result.exit_code, 0)
        self.assertIn("unknown host nope", result.output)
        self.assertFalse(fleet_paths.inventory_file.exists())

    def test_output_option_writes_to_the_given_path(self) -> None:
        fleet_paths = self._write_state([_record("scaly", 8807)])
        dest = self.root / "custom" / "fleet.toml"

        result = self._invoke(fleet_paths, ["--output", str(dest)])

        self.assertEqual(result.exit_code, 0, result.output)
        self.assertIn(str(dest), result.output)
        self.assertEqual(dest.read_text(), '[hosts]\nscaly = "http://127.0.0.1:8807"\n')


if __name__ == "__main__":
    unittest.main()
