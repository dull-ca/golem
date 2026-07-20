# golem docs site

Astro + Starlight. Lives at `docs/`. Source of truth for the public docs: the
site teaches deployment, while `emet/docs/adr/` records the engine's design
decisions.

## Run locally

Requires Node 20+ (or bun). The repo's `devenv.nix` enables Rust by default; if
you want everything in one shell, also `nix profile install nixpkgs#bun` or use
your system's node/bun.

```bash
cd docs
bun install        # or: npm install
bun run dev        # or: npm run dev
```

Then open http://localhost:4321.

## Layout

```
docs/
├─ astro.config.mjs        # sidebar + site config
├─ src/content/docs/
│  ├─ index.mdx            # landing
│  ├─ getting-started/     # install, first bundle
│  ├─ concepts/            # three layers, journal, trust
│  ├─ guides/              # tier 1 / 2 / 3 deployment walkthroughs
│  └─ reference/           # bundle format, CLI, status
└─ public/                 # static assets
```

## Editing rules

1. **Honesty about milestones.** If something only works in M2, label it. The
   `:::caution[M2]` aside is what we use.
2. **Examples track the repo.** Anything in a code block under `guides/` should
   correspond to something in `examples/` or `nickel/`. If you add a guide,
   add the example. If you change an example, update the guide.
3. **No unexplained jargon.** First use of "claim", "bundle", "intent",
   "capture" gets a one-line gloss. After that you can run.
