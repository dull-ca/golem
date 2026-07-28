import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

from fleet import vm
from fleet.config import HostPlan, Paths
from fleet.state import FleetState, VmRecord


class BringUpResumeTest(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = TemporaryDirectory()
        self.root = Path(self._tmp.name)
        self.paths = Paths(root=self.root)
        self.state = FleetState(self.paths.state_file)
        self.launched: list[Path] = []

        def fake_launch(vm_dir, plan, disk, seed):
            self.launched.append(Path(disk))
            pidfile = Path(vm_dir) / "qemu.pid"
            pidfile.write_text("999999999")
            return 999999999, pidfile, Path(vm_dir) / "console.log"

        self._real_launch = vm.launch_qemu
        self._real_wait = vm.wait_for_ssh
        self._real_seed = vm.build_seed_iso
        vm.launch_qemu = fake_launch
        vm.wait_for_ssh = lambda paths, record: None
        vm.build_seed_iso = lambda paths, vm_dir, name: Path(vm_dir) / "seed.iso"

    def tearDown(self) -> None:
        vm.launch_qemu = self._real_launch
        vm.wait_for_ssh = self._real_wait
        vm.build_seed_iso = self._real_seed
        self._tmp.cleanup()

    def _stopped_record(self, name: str) -> VmRecord:
        vm_dir = self.paths.vm_dir(name)
        vm_dir.mkdir(parents=True, exist_ok=True)
        disk = vm_dir / "disk.qcow2"
        disk.write_text("guest-marker")
        record = VmRecord(
            name=name,
            ssh_port=2245,
            golemd_port=8845,
            pid=2147483646,
            disk=str(disk),
            pidfile=str(vm_dir / "qemu.pid"),
            console_log=str(vm_dir / "console.log"),
            publish=[],
        )
        self.state.put(record)
        return record

    def test_resume_keeps_the_existing_disk(self) -> None:
        record = self._stopped_record("test-fleet-fix")
        disk = Path(record.disk)
        plan = HostPlan(name="test-fleet-fix", ssh_port=2299, golemd_port=8899)
        vm.bring_up(self.paths, self.state, plan, base_image=Path("/nonexistent.qcow2"))
        self.assertTrue(disk.exists())
        self.assertEqual(disk.read_text(), "guest-marker")

    def test_resume_keeps_recorded_ports_not_replan(self) -> None:
        self._stopped_record("test-fleet-fix")
        plan = HostPlan(name="test-fleet-fix", ssh_port=2299, golemd_port=8899)
        resumed = vm.bring_up(self.paths, self.state, plan, base_image=Path("/nonexistent.qcow2"))
        self.assertEqual(resumed.ssh_port, 2245)
        self.assertEqual(resumed.golemd_port, 8845)

    def test_resume_relaunches_against_the_same_disk(self) -> None:
        record = self._stopped_record("test-fleet-fix")
        plan = HostPlan(name="test-fleet-fix", ssh_port=2299, golemd_port=8899)
        vm.bring_up(self.paths, self.state, plan, base_image=Path("/nonexistent.qcow2"))
        self.assertEqual(self.launched, [Path(record.disk)])

    def test_resume_adds_new_publish_forwards(self) -> None:
        self._stopped_record("test-fleet-fix")
        plan = HostPlan(
            name="test-fleet-fix",
            ssh_port=2299,
            golemd_port=8899,
            publish=((8080, 80),),
        )
        resumed = vm.bring_up(
            self.paths, self.state, plan, base_image=Path("/nonexistent.qcow2")
        )
        self.assertEqual(resumed.publish, [(8080, 80)])

    def test_resume_merges_publish_forwards_without_duplicating(self) -> None:
        record = self._stopped_record("test-fleet-fix")
        record.publish = [(5000, 5000)]
        self.state.put(record)
        plan = HostPlan(
            name="test-fleet-fix",
            ssh_port=2299,
            golemd_port=8899,
            publish=((5000, 5000), (8080, 80)),
        )
        resumed = vm.bring_up(
            self.paths, self.state, plan, base_image=Path("/nonexistent.qcow2")
        )
        self.assertEqual(resumed.publish, [(5000, 5000), (8080, 80)])


if __name__ == "__main__":
    unittest.main()
