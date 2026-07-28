import unittest

from fleet import config, vm


class SeedUserDataTests(unittest.TestCase):
    def test_golem_user_joins_systemd_journal_group(self):
        data = vm._seed_user_data("scaly", "ssh-ed25519 AAAA test")
        self.assertIn(f"  - name: {config.GUEST_USER}", data)
        self.assertIn("    groups: [systemd-journal]", data)

    def test_pubkey_and_sudoer_are_preserved(self):
        data = vm._seed_user_data("scaly", "ssh-ed25519 AAAA test")
        self.assertIn("      - ssh-ed25519 AAAA test", data)
        self.assertIn("    sudo: ALL=(ALL) NOPASSWD:ALL", data)


if __name__ == "__main__":
    unittest.main()
