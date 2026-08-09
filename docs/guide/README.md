# Emet guide

Emet is a typed, functional language for describing machine configuration. A
program is `main : List Scroll` — one `Scroll` per host, a tree whose leaves each
hold a list of *glyphs* (apt packages, systemd units, files, lines in files).
That list of scrolls is the whole output. Emet writes it; the `golemd` daemon
enacts it on real hosts.

- **[Tutorial](tutorial.md)** — new to Emet? Build a fleet, one runnable program
  at a time.
- **[How-to](how-to.md)** — recipes for specific tasks.
- **[Reference](reference.md)** — glyphs, scrolls, types, modules, operators,
  syntax, secrets.
- **[Explanation](explanation.md)** — the mental model, in one page.

Every program shown compiles. Run one:

```
cargo run -p emet -- build --text apps/emet/examples/single-host.emet
```

Editor support lives in [`emet.nvim`](https://github.com/dull-ca/emet.nvim)
(tree-sitter highlighting + Neovim plugin) and `emet-lsp` (inference,
diagnostics).
