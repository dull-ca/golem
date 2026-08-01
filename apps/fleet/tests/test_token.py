import stat
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

from fleet.config import Paths


class EnsureTokenTests(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = TemporaryDirectory()
        self.paths = Paths(root=Path(self._tmp.name))
        self.addCleanup(self._tmp.cleanup)

    def test_creates_the_token_file_when_missing(self) -> None:
        from fleet.token import ensure_token

        self.assertFalse(self.paths.token_file.exists())
        token = ensure_token(self.paths)
        self.assertTrue(self.paths.token_file.exists())
        self.assertEqual(self.paths.token_file.read_text().strip(), token)

    def test_returns_a_nonempty_url_safe_looking_token(self) -> None:
        from fleet.token import ensure_token

        token = ensure_token(self.paths)
        self.assertGreater(len(token), 32)

    def test_is_idempotent_across_calls(self) -> None:
        from fleet.token import ensure_token

        first = ensure_token(self.paths)
        second = ensure_token(self.paths)
        self.assertEqual(first, second)

    def test_does_not_regenerate_an_existing_token(self) -> None:
        from fleet.token import ensure_token

        self.paths.fleet_dir.mkdir(parents=True, exist_ok=True)
        self.paths.token_file.write_text("existing-token")
        self.paths.token_file.chmod(0o600)

        token = ensure_token(self.paths)

        self.assertEqual(token, "existing-token")

    def test_the_token_file_is_chmod_0600(self) -> None:
        from fleet.token import ensure_token

        ensure_token(self.paths)
        mode = stat.S_IMODE(self.paths.token_file.stat().st_mode)
        self.assertEqual(mode, 0o600)


if __name__ == "__main__":
    unittest.main()
