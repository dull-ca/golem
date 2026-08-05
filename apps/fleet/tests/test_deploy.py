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

    def test_declares_the_secrets_key_file(self) -> None:
        text = deploy.golemd_config_toml()
        self.assertIn("[secrets]", text)
        self.assertIn(f'key_file = "{deploy.SECRET_KEY_REMOTE_PATH}"', text)

    def test_the_key_and_the_token_land_at_different_remote_paths(self) -> None:
        self.assertNotEqual(deploy.SECRET_KEY_REMOTE_PATH, deploy.TOKEN_REMOTE_PATH)


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
        subprocess_calls: list[tuple[list[str], dict]] = []

        def fake_ssh_check(paths, record, remote, input_text=None):
            remotes.append(remote)
            return ""

        def fake_run(argv, **kwargs):
            subprocess_calls.append((list(argv), kwargs))
            return subprocess.CompletedProcess(argv, 0, stdout="", stderr="")

        with (
            mock.patch.object(deploy.subprocess, "run", side_effect=fake_run),
            mock.patch.object(deploy, "_ssh_check", side_effect=fake_ssh_check),
        ):
            deploy.deploy_golemd(self.paths, self.record, self.binary)
        return remotes, subprocess_calls

    def test_ensures_the_shared_token_exists_locally(self) -> None:
        self.assertFalse(self.paths.token_file.exists())
        self._run_deploy()
        self.assertTrue(self.paths.token_file.exists())

    def test_writes_the_token_to_its_remote_path_root_owned_and_0600(self) -> None:
        remotes, subprocess_calls = self._run_deploy()
        token = self.paths.token_file.read_text().strip()
        token_write = [
            (argv, kwargs)
            for argv, kwargs in subprocess_calls
            if "tee" in argv and deploy.TOKEN_REMOTE_PATH in argv
        ]
        self.assertEqual(len(token_write), 1)
        argv, kwargs = token_write[0]
        self.assertIn(">", argv)
        self.assertIn("/dev/null", argv)
        self.assertEqual(kwargs.get("input"), token)
        self.assertTrue(
            any(r == ["sudo", "chmod", "0600", deploy.TOKEN_REMOTE_PATH] for r in remotes)
        )
        self.assertTrue(
            any(r == ["sudo", "chown", "root:root", deploy.TOKEN_REMOTE_PATH] for r in remotes)
        )

    def test_the_remote_token_is_created_0600_before_a_byte_reaches_it(self) -> None:
        _, subprocess_calls = self._run_deploy()
        touching_the_token = [
            argv for argv, _ in subprocess_calls if deploy.TOKEN_REMOTE_PATH in argv
        ]
        self.assertGreaterEqual(len(touching_the_token), 2)
        create, write = touching_the_token[0], touching_the_token[1]
        self.assertIn("install", create)
        self.assertEqual(create[create.index("-m") + 1], "0600")
        self.assertIn("/dev/null", create)
        self.assertNotIn("tee", create)
        self.assertIn("tee", write)

    def test_writes_the_golemd_config_naming_the_token_file(self) -> None:
        remotes, _ = self._run_deploy()
        self.assertTrue(
            any(r == ["sudo", "tee", deploy.CONFIG_REMOTE_PATH] for r in remotes)
        )

    def test_writes_the_unit_and_restarts_the_service(self) -> None:
        remotes, _ = self._run_deploy()
        self.assertTrue(any(r == ["sudo", "tee", deploy.SERVICE_REMOTE_PATH] for r in remotes))
        self.assertTrue(any(r == ["sudo", "systemctl", "daemon-reload"] for r in remotes))
        self.assertTrue(any(r == ["sudo", "systemctl", "restart", "golemd"] for r in remotes))

    def test_a_failing_token_write_never_leaks_the_token_in_the_error(self) -> None:
        def fake_run(argv, **kwargs):
            if "tee" in argv and deploy.TOKEN_REMOTE_PATH in argv:
                piped = kwargs.get("input", "")
                return subprocess.CompletedProcess(argv, 1, stdout=piped, stderr="")
            return subprocess.CompletedProcess(argv, 0, stdout="", stderr="")

        with mock.patch.object(deploy.subprocess, "run", side_effect=fake_run):
            with self.assertRaises(deploy.FleetError) as ctx:
                deploy.deploy_golemd(self.paths, self.record, self.binary)

        token = self.paths.token_file.read_text().strip()
        self.assertNotIn(token, str(ctx.exception))

    def test_ensures_the_fleet_secret_key_exists_locally(self) -> None:
        self.assertFalse(self.paths.secret_key_file.exists())
        self._run_deploy()
        self.assertTrue(self.paths.secret_key_file.exists())

    def test_writes_the_secret_key_to_its_remote_path_root_owned_and_0600(self) -> None:
        remotes, subprocess_calls = self._run_deploy()
        key = self.paths.secret_key_file.read_text().strip()
        key_write = [
            (argv, kwargs)
            for argv, kwargs in subprocess_calls
            if "tee" in argv and deploy.SECRET_KEY_REMOTE_PATH in argv
        ]
        self.assertEqual(len(key_write), 1)
        argv, kwargs = key_write[0]
        self.assertIn(">", argv)
        self.assertIn("/dev/null", argv)
        self.assertEqual(kwargs.get("input"), key)
        self.assertTrue(
            any(r == ["sudo", "chmod", "0600", deploy.SECRET_KEY_REMOTE_PATH] for r in remotes)
        )
        self.assertTrue(
            any(
                r == ["sudo", "chown", "root:root", deploy.SECRET_KEY_REMOTE_PATH]
                for r in remotes
            )
        )

    def test_the_remote_key_is_created_0600_before_a_byte_reaches_it(self) -> None:
        _, subprocess_calls = self._run_deploy()
        touching_the_key = [
            argv for argv, _ in subprocess_calls if deploy.SECRET_KEY_REMOTE_PATH in argv
        ]
        self.assertGreaterEqual(len(touching_the_key), 2)
        create, write = touching_the_key[0], touching_the_key[1]
        self.assertIn("install", create)
        self.assertEqual(create[create.index("-m") + 1], "0600")
        self.assertIn("/dev/null", create)
        self.assertNotIn("tee", create)
        self.assertIn("tee", write)

    def test_a_failing_key_write_never_leaks_the_key_in_the_error(self) -> None:
        def fake_run(argv, **kwargs):
            if "tee" in argv and deploy.SECRET_KEY_REMOTE_PATH in argv:
                piped = kwargs.get("input", "")
                return subprocess.CompletedProcess(argv, 1, stdout=piped, stderr="")
            return subprocess.CompletedProcess(argv, 0, stdout="", stderr="")

        with mock.patch.object(deploy.subprocess, "run", side_effect=fake_run):
            with self.assertRaises(deploy.FleetError) as ctx:
                deploy.deploy_golemd(self.paths, self.record, self.binary)

        key = self.paths.secret_key_file.read_text().strip()
        self.assertNotIn(key, str(ctx.exception))


class CompileManifestTests(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = TemporaryDirectory()
        self.paths = Paths(root=Path(self._tmp.name))
        self.addCleanup(self._tmp.cleanup)
        self.source = Path(self._tmp.name) / "fleet.emet"
        self.source.write_text("main : List Scroll\nmain = []\n")

    def test_hands_emetc_the_same_key_the_guests_are_given(self) -> None:
        captured: dict = {}

        def fake_run(argv, **kwargs):
            captured["argv"] = list(argv)
            captured["env"] = kwargs.get("env")
            out = Path(argv[argv.index("-o") + 1])
            out.parent.mkdir(parents=True, exist_ok=True)
            out.write_bytes(b"\x00")
            return subprocess.CompletedProcess(argv, 0, stdout="", stderr="")

        with mock.patch.object(deploy.subprocess, "run", side_effect=fake_run):
            deploy.compile_manifest(self.paths, self.source)

        key_file = str(self.paths.secret_key_file)
        self.assertEqual(captured["env"]["GOLEM_SECRET_KEY_FILE"], key_file)

    def test_the_key_never_reaches_the_emetc_argv(self) -> None:
        captured: dict = {}

        def fake_run(argv, **kwargs):
            captured["argv"] = list(argv)
            out = Path(argv[argv.index("-o") + 1])
            out.parent.mkdir(parents=True, exist_ok=True)
            out.write_bytes(b"\x00")
            return subprocess.CompletedProcess(argv, 0, stdout="", stderr="")

        with mock.patch.object(deploy.subprocess, "run", side_effect=fake_run):
            deploy.compile_manifest(self.paths, self.source)

        key = self.paths.secret_key_file.read_text().strip()
        self.assertNotIn(key, " ".join(captured["argv"]))


class SshWriteSecretTests(unittest.TestCase):
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

    def test_redirects_the_remote_tees_stdout_to_dev_null(self) -> None:
        captured = {}

        def fake_run(argv, **kwargs):
            captured["argv"] = list(argv)
            return subprocess.CompletedProcess(argv, 0, stdout="", stderr="")

        with mock.patch.object(deploy.subprocess, "run", side_effect=fake_run):
            deploy._ssh_write_secret(self.paths, self.record, "/etc/golem/token", "s3cret")

        self.assertIn(">", captured["argv"])
        self.assertIn("/dev/null", captured["argv"])

    def test_a_failing_write_never_includes_stdout_in_the_error(self) -> None:
        secret = "s3cret-token-value"

        def fake_run(argv, **kwargs):
            return subprocess.CompletedProcess(argv, 1, stdout=secret, stderr="")

        with mock.patch.object(deploy.subprocess, "run", side_effect=fake_run):
            with self.assertRaises(deploy.FleetError) as ctx:
                deploy._ssh_write_secret(self.paths, self.record, "/etc/golem/token", secret)

        self.assertNotIn(secret, str(ctx.exception))

    def test_a_failing_write_still_reports_stderr(self) -> None:
        def fake_run(argv, **kwargs):
            return subprocess.CompletedProcess(argv, 1, stdout="ignored", stderr="Permission denied")

        with mock.patch.object(deploy.subprocess, "run", side_effect=fake_run):
            with self.assertRaises(deploy.FleetError) as ctx:
                deploy._ssh_write_secret(self.paths, self.record, "/etc/golem/token", "whatever")

        self.assertIn("Permission denied", str(ctx.exception))


if __name__ == "__main__":
    unittest.main()
