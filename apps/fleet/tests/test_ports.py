import unittest

from fleet import config


class NameToSlotTest(unittest.TestCase):
    def test_slot_is_stable_per_name(self) -> None:
        first = config.slot_for_name("registry")
        second = config.slot_for_name("registry")
        self.assertEqual(first, second)

    def test_slots_are_collision_free_across_fleet_hosts(self) -> None:
        names = config.LICHESS_HOSTS + ["registry", "builder", "puller"]
        slots = [config.slot_for_name(name) for name in names]
        self.assertEqual(len(slots), len(set(slots)))

    def test_distinct_names_take_distinct_ports(self) -> None:
        for name in config.LICHESS_HOSTS + ["registry", "builder", "puller"]:
            plan = config.plan_hosts([name])[0]
            self.assertEqual(plan.ssh_port, config.SSH_PORT_BASE + config.slot_for_name(name))
            self.assertEqual(
                plan.golemd_port, config.GOLEMD_PORT_BASE + config.slot_for_name(name)
            )

    def test_single_host_boot_matches_group_boot_ports(self) -> None:
        group = {plan.name: plan for plan in config.plan_hosts(["registry", "builder", "puller"])}
        alone = config.plan_hosts(["builder"])[0]
        self.assertEqual(alone.ssh_port, group["builder"].ssh_port)
        self.assertEqual(alone.golemd_port, group["builder"].golemd_port)

    def test_ports_stay_within_slot_range(self) -> None:
        for name in config.LICHESS_HOSTS + ["registry", "builder", "puller"]:
            slot = config.slot_for_name(name)
            self.assertGreaterEqual(slot, 0)
            self.assertLess(slot, config.PORT_SLOT_COUNT)


if __name__ == "__main__":
    unittest.main()
