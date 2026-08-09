# emet-lsp

A language server for Emet, served from the compiler's own inference. Hover,
completion, go-to-definition, document symbols, and diagnostics all read one
`emet::Analysis` — the same types, scopes, and definition sites `emetc` computes
— so the editor and the compiler cannot disagree about a program (ADR 0018,
ADR 0037).

It is a separate crate from the `emet` core, so the core's dependency footprint
is unchanged. The server uses the lightweight synchronous LSP stack
(`lsp-server` + `lsp-types`), not `tower-lsp`/`tokio`. Each request re-analyzes
the buffer; on the largest example that costs about 70 ms, and there is no
cache.

## Build

From the workspace root:

```
cargo build -p emet-lsp
```

The binary is produced at:

```
target/debug/emet-lsp
```

For an optimized build use `cargo build -p emet-lsp --release`, which yields
`target/release/emet-lsp`. Use the absolute path to whichever you built when
configuring your editor. `nix profile install <checkout>#golem-tools` puts
`emet-lsp` on `PATH` everywhere instead, which is what an editor opened on
another repository needs (`QUICKSTART.md`).

## Capabilities

- `textDocumentSync = Full` — the client sends the whole document on each change.
- `textDocument/publishDiagnostics` after every `didOpen` and `didChange`,
  carrying every error the compiler reports. The parser recovers at declaration
  boundaries, so a buffer with several mistakes lights up at all of them rather
  than at the first (ADR 0022). A compile that succeeds publishes an empty list,
  clearing what was there.
- `textDocument/hover` — the inferred type of the expression under the cursor,
  or the declaration of the type name written there, together with the `--` doc
  block above its definition and, for an imported name, the module it came from.
  The four parser-built authoring types (`Scroll`, `Policy`, `Contents`,
  `OnExhaust`) hover with their authoring shape from a prelude-owned table.
- `textDocument/completion` — every name in scope at the cursor, each labelled
  with its rendered type.
- `textDocument/definition` — the definition site: a span in this file, or a
  span in the `.emet` file that exports the name.
- `textDocument/documentSymbol` — the buffer's definitions with their rendered
  types, nested, re-parsed on each request so the outline empties mid-edit
  rather than going stale.

## Project-aware analysis

A document with a file path is analyzed as the entry of its own import graph,
with the editor buffer standing in for the file on disk. Imports resolve over
the same search path `emetc` uses: the entry file's own directory, then each
`source-directories` entry of the nearest `emet.json` (ADR 0024).

```json
{ "source-directories": ["lib"] }
```

That is what makes hover, completion, and go-to-definition work on names a
module imports, and go-to-definition cross the file boundary into the library
that exports them. Diagnostics from the other modules in the graph are held
back — their spans index into their own files — and appear when you open those
files. A buffer with no path, and a graph that fails to load, fall back to
single-file analysis: types within the file still resolve, imported names do
not.

## Syntax highlighting

The language server publishes no tokens. Colour comes from the tree-sitter
grammar in the separate [emet.nvim](https://github.com/dull-ca/emet.nvim)
repository, installed by your editor (`:TSInstall! emet` in Neovim). The two are
independent: the grammar highlights without the server running, and the server
answers without the grammar installed.

## Neovim (built-in LSP)

Neovim ships an LSP client. The snippet below registers `emet` as a filetype
for `*.emet`, then launches `emet-lsp` for those buffers. Put it in your
`init.lua` and replace the `cmd` path with your built binary's absolute path.

```lua
vim.filetype.add({ extension = { emet = "emet" } })

vim.api.nvim_create_autocmd("FileType", {
  pattern = "emet",
  callback = function(args)
    vim.lsp.start({
      name = "emet-lsp",
      cmd = { "/absolute/path/to/target/debug/emet-lsp" },
      root_dir = vim.fs.dirname(vim.fs.find({ "emet.json", ".git", "Cargo.toml" }, {
        upward = true,
        path = vim.api.nvim_buf_get_name(args.buf),
      })[1]),
    })
  end,
})
```

Diagnostics then appear inline and in `:lua vim.diagnostic.setqflist()`. The
rest is `vim.lsp.buf`: `hover()`, `completion.get()`, `definition()`, and
`document_symbol()`, on whatever keys your config binds them to.

### nvim-lspconfig style

If you prefer `nvim-lspconfig`, define a custom server config:

```lua
local configs = require("lspconfig.configs")
local lspconfig = require("lspconfig")

vim.filetype.add({ extension = { emet = "emet" } })

if not configs.emet_lsp then
  configs.emet_lsp = {
    default_config = {
      cmd = { "/absolute/path/to/target/debug/emet-lsp" },
      filetypes = { "emet" },
      root_dir = lspconfig.util.root_pattern("emet.json", ".git", "Cargo.toml"),
      single_file_support = true,
    },
  }
end

lspconfig.emet_lsp.setup({})
```

## Vim (via a plugin)

Classic Vim needs an LSP plugin. One line of config for either:

- **vim-lsp**: `call lsp#register_server({'name': 'emet-lsp', 'cmd': {->['/absolute/path/to/target/debug/emet-lsp']}, 'allowlist': ['emet']})`
- **coc.nvim**: add to `coc-settings.json` under `languageserver`: `"emet": {"command": "/absolute/path/to/target/debug/emet-lsp", "filetypes": ["emet"]}`
