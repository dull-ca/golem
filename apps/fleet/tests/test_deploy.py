import subprocess
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest import mock

from fleet import deploy
from fleet.config import Paths
from fleet.state import VmRecord


class ServiceUnitTests(unittest.TestCase):
    def test_listens_on_loopback_only(self) -> None:
        unit = deploy.service_unit("scaly")
        self.assertIn(f"--listen 127.0.0.1:{deploy.GOLEMD_GUEST_PORT}", unit)
        self.assertNotIn("0.0.0.0", unit)

    def test_carries_the_config_flag(self) -> None:
        unit = deploy.service_unit("scaly")
        self.assertIn(f"--config {deploy.CONFIG_REMOTE_PATH}", unit)

    def test_carries_the_host_and_reconciler(self) -> None:
        unit = deploy.service_unit("scaly")
        self.assertIn("--host scaly", unit)
        self.assertIn("--reconciler host", unit)


class GolemdConfigTomlTests(unittest.TestCase):
    def test_declares_the_auth_token_file(self) -> None:
        text = deploy.golemd_config_toml()
        self.assertIn("[auth]", text)
        self.assertIn(f'token_file = "{deploy.TOKEN_REMOTE_PATH}"', text)


class DeployGolemdTests(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = TemporaryDirectory()
        self.paths = Paths(root=Path(self._tmp.name))
        self.addCleanup(self._tmp.cleanup)
        self.record = VmRecord(
            name="scaly",
            ssh_port=2245,
            golemd_port=8845,
            pid=1,
            disk="/dev/null",
            pidfile="/dev/null",
            console_log="/dev/null",
        )
        self.binary = Path(self._tmp.name) / "golemd"
        self.binary.write_bytes(b"\x00")

    def _run_deploy(self):
        remotes: list[list[str]] = []

        def fake_ssh_check(paths, record, remote, input_text=None):
            remotes.append(remote)
            return ""

        with (
            mock.patch.object(
                deploy.subprocess,
                "run",
                return_value=subprocess.CompletedProcess([], 0),
            ),
            mock.patch.object(deploy, "_ssh_check", side_effect=fake_ssh_check),
        ):
            deploy.deploy_golemd(self.paths, self.record, self.binary)
        return remotes

    def test_ensures_the_shared_token_exists_locally(self) -> None:
        self.assertFalse(self.paths.token_file.exists())
        self._run_deploy()
        self.assertTrue(self.paths.token_file.exists())

    def test_writes_the_token_to_its_remote_path_root_owned_and_0600(self) -> None:
        remotes = self._run_deploy()
        self.assertTrue(any(r == ["sudo", "tee", deploy.TOKEN_REMOTE_PATH] for r in remotes))
        self.assertTrue(
            any(r == ["sudo", "chmod", "0600", deploy.TOKEN_REMOTE_PATH] for r in remotes)
        )
        self.assertTrue(
            any(r == ["sudo", "chown", "root:root", deploy.TOKEN_REMOTE_PATH] for r in remotes)
        )

    def test_writes_the_golemd_config_naming_the_token_file(self) -> None:
        remotes = self._run_deploy()
        self.assertTrue(
            any(r == ["sudo", "tee", deploy.CONFIG_REMOTE_PATH] for r in remotes)
        )

    def test_writes_the_unit_and_restarts_the_service(self) -> None:
        remotes = self._run_deploy()
        self.assertTrue(any(r == ["sudo", "tee", deploy.SERVICE_REMOTE_PATH] for r in remotes))
        self.assertTrue(any(r == ["sudo", "systemctl", "daemon-reload"] for r in remotes))
        self.assertTrue(any(r == ["sudo", "systemctl", "restart", "golemd"] for r in remotes))


if __name__ == "__main__":
    unittest.main()
