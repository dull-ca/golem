import unittest
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest import mock

import httpx

from fleet import golemd_client, tunnel
from fleet.config import Paths
from fleet.state import VmRecord


def _record() -> VmRecord:
    return VmRecord(
        name="scaly",
        ssh_port=2245,
        golemd_port=8845,
        pid=1,
        disk="/dev/null",
        pidfile="/dev/null",
        console_log="/dev/null",
    )


class StatusTests(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = TemporaryDirectory()
        self.paths = Paths(root=Path(self._tmp.name))
        self.addCleanup(self._tmp.cleanup)

    def test_returns_the_tunneled_json_on_success(self) -> None:
        with mock.patch.object(tunnel, "get_json", return_value={"host": "scaly"}) as fake:
            result = golemd_client.status(self.paths, _record(), "s3cret")
        self.assertEqual(result, {"host": "scaly"})
        args, kwargs = fake.call_args
        self.assertEqual(args[0], self.paths)
        self.assertEqual(args[2], "status")
        self.assertEqual(args[3], "s3cret")

    def test_returns_none_when_the_tunnel_fails(self) -> None:
        with mock.patch.object(tunnel, "get_json", side_effect=tunnel.TunnelError("nope")):
            result = golemd_client.status(self.paths, _record(), "s3cret")
        self.assertIsNone(result)

    def test_returns_none_on_an_http_error(self) -> None:
        request = httpx.Request("GET", "http://127.0.0.1:1/status")
        with mock.patch.object(
            tunnel,
            "get_json",
            side_effect=httpx.HTTPStatusError("boom", request=request, response=httpx.Response(500, request=request)),
        ):
            result = golemd_client.status(self.paths, _record(), "s3cret")
        self.assertIsNone(result)


class StateTests(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = TemporaryDirectory()
        self.paths = Paths(root=Path(self._tmp.name))
        self.addCleanup(self._tmp.cleanup)

    def test_returns_the_tunneled_json_on_success(self) -> None:
        with mock.patch.object(tunnel, "get_json", return_value={"content_id": "abc"}):
            result = golemd_client.state(self.paths, _record(), "s3cret")
        self.assertEqual(result, {"content_id": "abc"})

    def test_returns_none_when_the_tunnel_fails(self) -> None:
        with mock.patch.object(tunnel, "get_json", side_effect=tunnel.TunnelError("nope")):
            result = golemd_client.state(self.paths, _record(), "s3cret")
        self.assertIsNone(result)


if __name__ == "__main__":
    unittest.main()
