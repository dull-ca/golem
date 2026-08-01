import unittest
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest import mock

from typer.testing import CliRunner

from fleet import inventory
from fleet.config import Paths
from fleet.state import FleetState, VmRecord


def _record(name: str, ssh_port: int = 2200, golemd_port: int = 8800) -> VmRecord:
    return VmRecord(
        name=name,
        ssh_port=ssh_port,
        golemd_port=golemd_port,
        pid=1,
        disk="/dev/null",
        pidfile="/dev/null",
        console_log="/dev/null",
    )


class HostEntryTests(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = TemporaryDirectory()
        self.paths = Paths(root=Path(self._tmp.name))
        self.addCleanup(self._tmp.cleanup)

    def test_ssh_destination_is_the_golem_user_on_loopback(self) -> None:
        entry = inventory.host_entry(self.paths, _record("scaly", ssh_port=2245))
        self.assertEqual(entry.ssh, "golem@127.0.0.1")
        self.assertEqual(entry.ssh_port, 2245)

    def test_ssh_args_carry_the_fleet_key_as_an_absolute_path(self) -> None:
        entry = inventory.host_entry(self.paths, _record("scaly"))
        self.assertEqual(entry.ssh_args[0], "-i")
        self.assertEqual(Path(entry.ssh_args[1]), self.paths.ssh_key.resolve())
        self.assertTrue(Path(entry.ssh_args[1]).is_absolute())

    def test_ssh_args_carry_the_standard_host_checking_options(self) -> None:
        entry = inventory.host_entry(self.paths, _record("scaly"))
        self.assertEqual(
            entry.ssh_args[2:],
            [
                "-o",
                "StrictHostKeyChecking=no",
                "-o",
                "UserKnownHostsFile=/dev/null",
                "-o",
                "LogLevel=ERROR",
            ],
        )

    def test_token_file_is_the_absolute_fleet_token_path(self) -> None:
        entry = inventory.host_entry(self.paths, _record("scaly"))
        self.assertEqual(Path(entry.token_file), self.paths.token_file.resolve())
        self.assertTrue(Path(entry.token_file).is_absolute())

    def test_remote_port_is_unset_by_default(self) -> None:
        entry = inventory.host_entry(self.paths, _record("scaly"))
        self.assertIsNone(entry.remote_port)


class InventoryEntriesTests(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = TemporaryDirectory()
        self.paths = Paths(root=Path(self._tmp.name))
        self.addCleanup(self._tmp.cleanup)

    def test_maps_each_record_to_its_own_entry(self) -> None:
        records = [_record("scaly", ssh_port=2201), _record("manta", ssh_port=2202)]
        entries = inventory.inventory_entries(self.paths, records)
        self.assertEqual([entry.name for entry in entries], ["scaly", "manta"])
        self.assertEqual(entries[0].ssh_port, 2201)
        self.assertEqual(entries[1].ssh_port, 2202)

    def test_preserves_the_given_record_order(self) -> None:
        records = [_record("remora"), _record("kaiju")]
        entries = inventory.inventory_entries(self.paths, records)
        self.assertEqual([entry.name for entry in entries], ["remora", "kaiju"])


class RenderHostsTomlTests(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = TemporaryDirectory()
        self.paths = Paths(root=Path(self._tmp.name))
        self.addCleanup(self._tmp.cleanup)

    def _entries(self, names: list[str]) -> list[inventory.HostEntry]:
        return inventory.inventory_entries(
            self.paths, [_record(name, ssh_port=2200 + i) for i, name in enumerate(names)]
        )

    def test_renders_one_table_header_per_entry(self) -> None:
        toml = inventory.render_hosts_toml(self._entries(["scaly"]))
        self.assertEqual(toml.splitlines()[0], "[hosts.scaly]")

    def test_renders_the_ssh_destination_and_port(self) -> None:
        toml = inventory.render_hosts_toml(self._entries(["scaly"]))
        self.assertIn('ssh = "golem@127.0.0.1"', toml)
        self.assertIn("ssh_port = 2200", toml)

    def test_renders_the_ssh_args_as_a_quoted_array(self) -> None:
        toml = inventory.render_hosts_toml(self._entries(["scaly"]))
        key_path = str(self.paths.ssh_key.resolve())
        self.assertIn(
            f'ssh_args = ["-i", "{key_path}", '
            '"-o", "StrictHostKeyChecking=no", '
            '"-o", "UserKnownHostsFile=/dev/null", '
            '"-o", "LogLevel=ERROR"]',
            toml,
        )

    def test_renders_the_token_file_path(self) -> None:
        toml = inventory.render_hosts_toml(self._entries(["scaly"]))
        token_path = str(self.paths.token_file.resolve())
        self.assertIn(f'token_file = "{token_path}"', toml)

    def test_omits_remote_port_when_it_is_the_default(self) -> None:
        toml = inventory.render_hosts_toml(self._entries(["scaly"]))
        self.assertNotIn("remote_port", toml)

    def test_includes_remote_port_when_it_departs_from_the_default(self) -> None:
        entry = inventory.host_entry(self.paths, _record("scaly"))
        entry.remote_port = 9000
        toml = inventory.render_hosts_toml([entry])
        self.assertIn("remote_port = 9000", toml)

    def test_renders_one_block_per_entry_in_order(self) -> None:
        toml = inventory.render_hosts_toml(self._entries(["remora", "kaiju"]))
        headers = [line for line in toml.splitlines() if line.startswith("[hosts.")]
        self.assertEqual(headers, ["[hosts.remora]", "[hosts.kaiju]"])

    def test_empty_entries_renders_an_empty_string(self) -> None:
        self.assertEqual(inventory.render_hosts_toml([]), "")

    def test_quotes_a_name_that_is_not_a_bare_toml_key(self) -> None:
        entries = inventory.inventory_entries(self.paths, [_record("has space")])
        toml = inventory.render_hosts_toml(entries)
        self.assertIn('[hosts."has space"]', toml)


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
        fleet_paths = self._write_state([_record("scaly", ssh_port=2245), _record("manta", ssh_port=2246)])

        result = self._invoke(fleet_paths, [])

        self.assertEqual(result.exit_code, 0, result.output)
        dest = fleet_paths.inventory_file
        self.assertIn(str(dest), result.output)
        written = dest.read_text()
        self.assertIn("[hosts.scaly]", written)
        self.assertIn("[hosts.manta]", written)
        self.assertIn("ssh_port = 2245", written)
        self.assertIn("ssh_port = 2246", written)

    def test_ensures_the_shared_token_exists(self) -> None:
        fleet_paths = self._write_state([_record("scaly")])

        self._invoke(fleet_paths, [])

        self.assertTrue(fleet_paths.token_file.exists())

    def test_hosts_option_filters_to_the_named_subset(self) -> None:
        fleet_paths = self._write_state([_record("scaly", ssh_port=2245), _record("manta", ssh_port=2246)])

        result = self._invoke(fleet_paths, ["--hosts", "manta"])

        self.assertEqual(result.exit_code, 0, result.output)
        written = fleet_paths.inventory_file.read_text()
        self.assertIn("[hosts.manta]", written)
        self.assertNotIn("[hosts.scaly]", written)

    def test_unknown_host_name_errors_clearly(self) -> None:
        fleet_paths = self._write_state([_record("scaly")])

        result = self._invoke(fleet_paths, ["--hosts", "nope"])

        self.assertNotEqual(result.exit_code, 0)
        self.assertIn("unknown host nope", result.output)
        self.assertFalse(fleet_paths.inventory_file.exists())

    def test_output_option_writes_to_the_given_path(self) -> None:
        fleet_paths = self._write_state([_record("scaly")])
        dest = self.root / "custom" / "fleet.toml"

        result = self._invoke(fleet_paths, ["--output", str(dest)])

        self.assertEqual(result.exit_code, 0, result.output)
        self.assertIn(str(dest), result.output)
        self.assertIn("[hosts.scaly]", dest.read_text())


if __name__ == "__main__":
    unittest.main()
