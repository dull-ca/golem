# emet-lsp

A minimal language server for Emet. It provides live diagnostics: on every
open and edit of a `.emet` document it runs the full `emet` compiler and
publishes parse, type, and analysis errors to your editor.

It is a separate crate from the `emet` core, so the core's dependency
footprint is unchanged. The server uses the lightweight synchronous LSP stack
(`lsp-server` + `lsp-types`), not `tower-lsp`/`tokio`.

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
configuring your editor.

## Capabilities

- `textDocumentSync = Full` (the client sends the whole document on each change).
- `textDocument/publishDiagnostics` after every `didOpen` and `didChange`. A
  failed compile publishes exactly one error diagnostic at the offending span;
  a successful compile publishes an empty list, clearing previous errors.

No completion, hover, or go-to-definition yet.

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
      root_dir = vim.fs.dirname(vim.fs.find({ ".git", "Cargo.toml" }, {
        upward = true,
        path = vim.api.nvim_buf_get_name(args.buf),
      })[1]),
    })
  end,
})
```

Diagnostics then appear inline and in `:lua vim.diagnostic.setqflist()`.

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
      root_dir = lspconfig.util.root_pattern(".git", "Cargo.toml"),
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
