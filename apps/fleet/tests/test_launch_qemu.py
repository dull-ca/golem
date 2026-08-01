import unittest
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest import mock

from fleet import config, vm
from fleet.config import HostPlan


class LaunchQemuArgvTests(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = TemporaryDirectory()
        self.vm_dir = Path(self._tmp.name)
        self.plan = HostPlan(name="test-fleet-fix", ssh_port=2299, golemd_port=8899)

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def _netdev_arg(self) -> str:
        captured: list[list[str]] = []

        def fake_run(argv, check=None):
            captured.append(argv)
            (self.vm_dir / "qemu.pid").write_text("42")

        with mock.patch.object(vm.subprocess, "run", fake_run):
            vm.launch_qemu(
                self.vm_dir,
                self.plan,
                disk=self.vm_dir / "disk.qcow2",
                seed=self.vm_dir / "seed.iso",
            )
        argv = captured[0]
        return argv[argv.index("-netdev") + 1]

    def test_argv_forwards_ssh(self) -> None:
        netdev = self._netdev_arg()
        self.assertIn(f"hostfwd=tcp:127.0.0.1:{self.plan.ssh_port}-:22", netdev)

    def test_argv_has_no_forward_to_guest_golemd_port(self) -> None:
        netdev = self._netdev_arg()
        self.assertNotIn(f"-:{config.GOLEMD_GUEST_PORT}", netdev)
        self.assertNotIn(str(self.plan.golemd_port), netdev)


if __name__ == "__main__":
    unittest.main()
