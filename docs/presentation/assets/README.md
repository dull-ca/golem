# Vendored assets

Files here are committed so the build never touches the network. `build.py` reads
them from disk, base64-encodes them into an Excalidraw `files` entry, and stamps
`created` and `lastRetrieved` with `scene.UPDATED` rather than a clock, so two
builds stay byte-identical.

## `robot-golem.svg`

golem's mark, used on the golem deck's title slide and on the slide where golem
has converged the fleet.

- **Author:** Lorc
- **Source:** <https://game-icons.net/1x1/lorc/robot-golem.html>
- **Licence:** [CC BY 3.0](https://creativecommons.org/licenses/by/3.0/)
- **Retrieved as:** the black-on-transparent SVG,
  `https://game-icons.net/icons/000000/transparent/1x1/lorc/robot-golem.svg`
- **Modifications:** none — the bytes are as served.

CC BY 3.0 requires attribution wherever the mark appears. It is credited in four
places, and all four have to stay: this file, `SPEC.md`, `README.md`, and a line
on each slide that draws it (`decks/golem/golem_symbol.py` holds the wording, so
there is one string to change). A slide that adds the mark adds the credit with
it.
