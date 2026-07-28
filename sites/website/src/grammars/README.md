# Vendored grammars

`emet.tmLanguage.json` is the TextMate grammar that Expressive Code / Shiki uses
to highlight Emet code blocks on this site (imported by `astro.config.mjs`).

It is a **committed, vendored** artifact — the site build does not generate it.
It is regenerated in the [emet.nvim](https://codeberg.org/dull/emet.nvim)
repository from the tree-sitter grammar's `grammar.js`:

```sh
# in a checkout of emet.nvim
node scripts/generate-textmate.mjs /path/to/golem/sites/website/src/grammars/emet.tmLanguage.json
```

When the Emet grammar changes, regenerate there and commit the updated JSON here.
