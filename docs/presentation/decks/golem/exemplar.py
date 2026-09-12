from __future__ import annotations

from typing import NamedTuple

from .glyph_kinds import APT, FILESYSTEM, LINE, SYSTEMD, Kind

PROGRAM = """main : List Scroll
main =
  [ scroll
      { name = "web"
      , glyphs =
          [ aptPackage { name = "nginx" }
          , directory { path = "/srv/www", mode = "0755" }
          , lineInFile { path = "/etc/hosts", line = "10.0.0.7 web" }
          , systemdService { unit = "nginx.service" }
          ]
      }
  ]"""

HOST = "web"


class Entry(NamedTuple):
    spelling: str
    target: str
    inverse: str
    kind: Kind


GLYPHS: tuple[Entry, ...] = (
    Entry("aptPackage", "nginx", "RemoveAptPackage", APT),
    Entry("directory", "/srv/www", "RemoveDirectory", FILESYSTEM),
    Entry("lineInFile", "/etc/hosts", "RemoveLineInFile", LINE),
    Entry("systemdService", "nginx.service", "DisableSystemdService", SYSTEMD),
)

WITHDRAWN = GLYPHS[0]
