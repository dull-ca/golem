# Emet guide

Emet is a typed, functional language for describing machine configuration. A
program is `main : List Scroll` — one `Scroll` per host, each a list of *glyphs*
(apt packages, systemd units, files). That list of scrolls is the whole output.
Emet writes it; the `golemd` daemon enacts it on real hosts.

- **[Tutorial](tutorial.md)** — new to Emet? Build a fleet, one runnable program
  at a time.
- **[How-to](how-to.md)** — recipes for specific tasks.
- **[Reference](reference.md)** — glyphs, types, prelude, operators, syntax.
- **[Explanation](explanation.md)** — the mental model, in one page.

Every program shown compiles. Run one:

```
cargo run -- examples/single-host.emet
```

Editor support lives in [`emet.nvim`](https://codeberg.org/dull/emet.nvim)
(tree-sitter highlighting + Neovim plugin) and `emet-lsp` (inference,
diagnostics).
