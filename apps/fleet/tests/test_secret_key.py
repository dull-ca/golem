import stat
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest import mock

from fleet.config import Paths
from fleet.vm import FleetError

KEY_HEX_CHARACTERS = 128


class EnsureSecretKeyTests(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = TemporaryDirectory()
        self.paths = Paths(root=Path(self._tmp.name))
        self.addCleanup(self._tmp.cleanup)

    def test_creates_the_key_file_when_missing(self) -> None:
        from fleet.token import ensure_secret_key

        self.assertFalse(self.paths.secret_key_file.exists())
        key = ensure_secret_key(self.paths)
        self.assertTrue(self.paths.secret_key_file.exists())
        self.assertEqual(self.paths.secret_key_file.read_text().strip(), key)

    def test_is_128_hexadecimal_characters(self) -> None:
        from fleet.token import ensure_secret_key

        key = ensure_secret_key(self.paths)
        self.assertEqual(len(key), KEY_HEX_CHARACTERS)
        int(key, 16)

    def test_is_idempotent_across_calls(self) -> None:
        from fleet.token import ensure_secret_key

        self.assertEqual(ensure_secret_key(self.paths), ensure_secret_key(self.paths))

    def test_does_not_regenerate_an_existing_key(self) -> None:
        from fleet.token import ensure_secret_key

        existing = "ab" * 64
        self.paths.fleet_dir.mkdir(parents=True, exist_ok=True)
        self.paths.secret_key_file.write_text(existing)
        self.paths.secret_key_file.chmod(0o600)

        self.assertEqual(ensure_secret_key(self.paths), existing)

    def test_the_key_file_is_chmod_0600(self) -> None:
        from fleet.token import ensure_secret_key

        ensure_secret_key(self.paths)
        mode = stat.S_IMODE(self.paths.secret_key_file.stat().st_mode)
        self.assertEqual(mode, 0o600)

    def test_the_key_file_is_never_world_readable_even_for_an_instant(self) -> None:
        import os

        from fleet.token import ensure_secret_key

        opened: list[int] = []
        real_open = os.open

        def recording_open(path, flags, mode=0o777, **kwargs):  # type: ignore[no-untyped-def]
            opened.append(mode)
            return real_open(path, flags, mode, **kwargs)

        with mock.patch.object(os, "open", recording_open):
            ensure_secret_key(self.paths)

        self.assertIn(0o600, opened)

    def test_an_empty_key_file_is_an_error_naming_the_path_and_the_fix(self) -> None:
        from fleet.token import ensure_secret_key

        self.paths.fleet_dir.mkdir(parents=True, exist_ok=True)
        self.paths.secret_key_file.write_text("   \n")

        with self.assertRaises(FleetError) as caught:
            ensure_secret_key(self.paths)

        self.assertIn(str(self.paths.secret_key_file), str(caught.exception))
        self.assertIn("delete", str(caught.exception))

    def test_a_malformed_key_file_is_an_error_naming_the_path(self) -> None:
        from fleet.token import ensure_secret_key

        self.paths.fleet_dir.mkdir(parents=True, exist_ok=True)
        self.paths.secret_key_file.write_text("not-hexadecimal\n")

        with self.assertRaises(FleetError) as caught:
            ensure_secret_key(self.paths)

        self.assertIn(str(self.paths.secret_key_file), str(caught.exception))

    def test_the_key_and_the_token_are_different_files(self) -> None:
        self.assertNotEqual(self.paths.secret_key_file, self.paths.token_file)


if __name__ == "__main__":
    unittest.main()
