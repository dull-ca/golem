# tree-sitter-emet

A [tree-sitter](https://tree-sitter.github.io) grammar for the
[Emet](../CLAUDE.md) configuration language, providing syntax highlighting
in Neovim via [nvim-treesitter](https://github.com/nvim-treesitter/nvim-treesitter).

It covers Emet's Elm-like surface syntax: line comments, string literals with
escapes and `${…}` interpolation, integer/float numbers, lower/upper
identifiers, qualified prelude names (`List.map`, `Maybe.withDefault`,
`String.fromInt`), keywords (`let in if then else case of`), the reserved glyph
constructors (`aptPackage systemdService file lineInFile scroll`), the full
operator table, lambdas, `let … in`, `case … of`, `if … then … else`, records
and field access, lists, function application, type signatures, and type
expressions including record types `{ r | field : a }`.

## Layout handling

Emet is layout-sensitive (offside rule). This grammar does **not** reimplement
the compiler's full layout algorithm — it only needs a robust, error-tolerant
tree for highlighting. A tiny external scanner (`src/scanner.c`) emits two
synthetic tokens:

- a **declaration boundary** when a new line starts at column 0, which ends the
  previous top-level declaration; and
- a **line boundary** inside `case … of`, which separates one arm from the next
  (Emet lays out case arms one per line).

Everything else relies on tree-sitter's error recovery, which is sufficient for
highlighting.

## String interpolation

A `string` node is a sequence of `string_content`, `escape_sequence`, and
`interpolation` nodes. An `interpolation` wraps a full Emet expression between
`interpolation_start` (`${`) and `interpolation_end` (`}`) nodes, so the
embedded expression is parsed and highlighted like any other code — including
nested strings, applications, and parenthesised sub-expressions. The `${` and
`}` delimiters are highlighted with `@punctuation.special` to set them apart
from the surrounding string.

## Requirements

Tree-sitter highlighting needs **Neovim** (0.9+) with the
[nvim-treesitter](https://github.com/nvim-treesitter/nvim-treesitter) plugin.
Classic Vim has no tree-sitter support; use Neovim.

You also need a C compiler on `PATH` (nvim-treesitter compiles the parser,
including this grammar's external scanner, when you install it).

## Install with nvim-treesitter

1. Register emet as a custom parser pointing at this directory, and map the
   `.emet` file type. Add to your Neovim config (e.g. `init.lua`):

   ```lua
   local parser_config = require("nvim-treesitter.parsers").get_parser_configs()

   parser_config.emet = {
     install_info = {
       -- Absolute path to THIS directory, or a git url + optional subdirectory.
       url = "/home/lakin/personal-repos/golem/emet/tree-sitter-emet",
       files = { "src/parser.c", "src/scanner.c" },
       branch = "main",
     },
     filetype = "emet",
   }

   -- Associate the .emet extension with the emet filetype.
   vim.filetype.add({ extension = { emet = "emet" } })
   ```

2. Install and compile the parser:

   ```vim
   :TSInstall emet
   ```

   (or `:TSInstallFromGrammar emet` when installing from the local `install_info`).

3. Install the highlight queries. nvim-treesitter looks for queries under a
   `queries/emet/` directory on your `runtimepath`. Symlink or copy the query
   files from here:

   ```sh
   mkdir -p ~/.config/nvim/queries/emet
   ln -s /home/lakin/personal-repos/golem/emet/tree-sitter-emet/queries/highlights.scm  ~/.config/nvim/queries/emet/highlights.scm
   ln -s /home/lakin/personal-repos/golem/emet/tree-sitter-emet/queries/injections.scm  ~/.config/nvim/queries/emet/injections.scm
   ln -s /home/lakin/personal-repos/golem/emet/tree-sitter-emet/queries/locals.scm      ~/.config/nvim/queries/emet/locals.scm
   ```

4. Open any `.emet` file. If highlighting is not active, run
   `:TSBufEnable highlight`.

## Queries

- `queries/highlights.scm` — maps nodes to standard capture names
  (`@comment`, `@string`, `@string.escape`, `@punctuation.special`, `@number`,
  `@keyword`, `@keyword.conditional`, `@keyword.function`, `@function`,
  `@function.builtin`, `@function.call`, `@type`, `@type.parameter`,
  `@constructor`, `@variable`, `@variable.parameter`, `@property`, `@operator`,
  `@punctuation.bracket`, `@punctuation.delimiter`).
- `queries/injections.scm` — treats `--` comments as comment text.
- `queries/locals.scm` — scopes and definitions for local variable resolution.

## Developing

```sh
tree-sitter generate          # regenerate src/parser.c from grammar.js
tree-sitter test              # run test/corpus
tree-sitter parse FILE.emet   # inspect a parse tree

# Parse every real example (expect zero ERROR nodes):
for f in ../crates/emet/examples/*.emet; do tree-sitter parse "$f"; done
```
