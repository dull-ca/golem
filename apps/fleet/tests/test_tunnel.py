import socket
import subprocess
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest import mock

from fleet import tunnel
from fleet.config import Paths
from fleet.state import VmRecord


def _record(ssh_port: int = 2245) -> VmRecord:
    return VmRecord(
        name="scaly",
        ssh_port=ssh_port,
        golemd_port=8845,
        pid=1,
        disk="/dev/null",
        pidfile="/dev/null",
        console_log="/dev/null",
    )


class SshForwardArgvTests(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = TemporaryDirectory()
        self.paths = Paths(root=Path(self._tmp.name))
        self.addCleanup(self._tmp.cleanup)

    def test_forwards_the_local_port_to_the_guests_loopback_remote_port(self) -> None:
        argv = tunnel.ssh_forward_argv(self.paths, _record(), local_port=9999, remote_port=7474)
        self.assertIn("-N", argv)
        self.assertIn("-L", argv)
        self.assertEqual(argv[argv.index("-L") + 1], "9999:127.0.0.1:7474")

    def test_carries_the_fleet_key_and_the_vms_ssh_port(self) -> None:
        argv = tunnel.ssh_forward_argv(self.paths, _record(ssh_port=2277), local_port=9999, remote_port=7474)
        self.assertIn("-i", argv)
        self.assertEqual(argv[argv.index("-i") + 1], str(self.paths.ssh_key))
        self.assertIn("-p", argv)
        self.assertEqual(argv[argv.index("-p") + 1], "2277")

    def test_carries_the_standard_host_checking_options(self) -> None:
        argv = tunnel.ssh_forward_argv(self.paths, _record(), local_port=9999, remote_port=7474)
        joined = " ".join(argv)
        self.assertIn("StrictHostKeyChecking=no", joined)
        self.assertIn("UserKnownHostsFile=/dev/null", joined)
        self.assertIn("LogLevel=ERROR", joined)

    def test_targets_the_golem_user_on_loopback(self) -> None:
        argv = tunnel.ssh_forward_argv(self.paths, _record(), local_port=9999, remote_port=7474)
        self.assertEqual(argv[-1], "golem@127.0.0.1")


class FreeLocalPortTests(unittest.TestCase):
    def test_returns_a_port_nothing_is_bound_to(self) -> None:
        port = tunnel.free_local_port()
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
            sock.bind(("127.0.0.1", port))


class GetJsonTests(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = TemporaryDirectory()
        self.paths = Paths(root=Path(self._tmp.name))
        self.addCleanup(self._tmp.cleanup)

    def test_terminates_and_waits_the_popen_even_on_success(self) -> None:
        fake_proc = mock.Mock()
        fake_proc.terminate = mock.Mock()
        fake_proc.wait = mock.Mock()

        fake_response = mock.Mock()
        fake_response.raise_for_status = mock.Mock()
        fake_response.json.return_value = {"ok": True}

        with (
            mock.patch.object(tunnel.subprocess, "Popen", return_value=fake_proc),
            mock.patch.object(tunnel, "wait_for_local_port", return_value=True),
            mock.patch.object(tunnel.httpx, "get", return_value=fake_response) as fake_get,
        ):
            result = tunnel.get_json(self.paths, _record(), "status", "s3cret")

        self.assertEqual(result, {"ok": True})
        fake_proc.terminate.assert_called_once()
        fake_proc.wait.assert_called_once()
        headers = fake_get.call_args.kwargs["headers"]
        self.assertEqual(headers["Authorization"], "Bearer s3cret")

    def test_terminates_and_waits_the_popen_even_on_failure(self) -> None:
        fake_proc = mock.Mock()
        fake_proc.terminate = mock.Mock()
        fake_proc.wait = mock.Mock()

        with (
            mock.patch.object(tunnel.subprocess, "Popen", return_value=fake_proc),
            mock.patch.object(tunnel, "wait_for_local_port", return_value=False),
        ):
            with self.assertRaises(tunnel.TunnelError):
                tunnel.get_json(self.paths, _record(), "status", "s3cret")

        fake_proc.terminate.assert_called_once()
        fake_proc.wait.assert_called_once()


if __name__ == "__main__":
    unittest.main()
