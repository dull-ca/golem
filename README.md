# golem

Golem keeps a small fleet of Debian machines in the state you describe.

You write that description in **Emet**, a typed functional language: generics,
records, pattern matching, exhaustiveness checking. `emetc` runs your program to
completion on your own machine and writes the result as a **manifest**. Every
function has been applied and every value computed by the time it leaves your
laptop, so a host receives desired state that already type-checked and already
ran.

The manifest holds one **scroll** per host: a tree whose leaves each carry a
handful of **glyphs**, one OS resource apiece. A leaf is the unit golem enacts —
it retries on its own schedule and settles whether or not its siblings do — and
the branches above it group leaves by subsystem, handing down retry settings and
reload obligations. Every scroll is identified by a BLAKE3 hash of its own bytes,
which is how the agent tells what changed and leaves the rest of the host alone.

There are four kinds of glyph: `aptPackage`, `systemdService`, the filesystem
glyph (`file`, `directory`, `symlink`), and `lineInFile`. Anything larger — a
container workload, a service behind a firewall, an ingress — is an Emet function
that returns glyphs. A few ship with the toolchain in [`lib/`](lib/); you write
the rest for your own fleet and reuse them across hosts. All of it lowers onto
those four kinds, which is what keeps the agent small.

`golemd` is the agent on each host. It reads the manifest, picks out its own
scroll, compares it against a journal of what it applied last time, and enacts
the difference. Before touching anything it records the state it is about to
change, so it can put that state back exactly; and it reverses only its own
edits, leaving whatever was on the box before it arrived alone.

```
your .emet  ──emetc──▶  manifest  ──golemctl──▶  golemd  ──▶  the box
```

Evaluating up front and reversing afterward are what make a change cheap to try.
Suppose state X works and you edit it into X+1. If a unit of X+1 fails and its
policy says roll back, which is the default, that unit returns to the state it
held under X — a working one — while the rest of the host settles on X+1. You can
try a change without being sure of it first.

## Start here

- **[QUICKSTART.md](QUICKSTART.md)** — build the binaries, run an agent, write a
  fleet in Emet, and apply it. The shortest path to something running.
- **[sites/website/](sites/website/)** — the public docs site (Astro +
  Starlight): getting started, guides, hands-on tutorials, language and format
  reference. `cd sites/website && bun install && bun run dev` serves it at
  <http://localhost:4321>.
- **[TUTORIAL-fleet.md](TUTORIAL-fleet.md)** — the same material against real
  Debian VMs on your workstation, with golemd's real reconcilers. The harness
  that boots them is [`apps/fleet/`](apps/fleet/README.md).

## Going deeper

- **[docs/guide/](docs/guide/)** — the Emet language on its own: a tutorial, task
  recipes, a reference for the glyphs and the type system, and the mental model
  in one page.
- **[docs/adr/](docs/adr/)** — every design decision, numbered and dated. The
  binary manifest (0012, 0013), golemd's glyph reconciliation (0014), the
  reversible reconcilers (0015), the recursive scroll and failure isolation
  (0031), and typed secrets on the wire (0047) are the load-bearing ones.
- **[libs/scroll-format/](libs/scroll-format/)** — the wire model itself,
  compiled into both `emetc` (the writer) and `golemd` (the reader), so the two
  ends cannot disagree about what a manifest means.
- **[examples/](examples/)** — worked fleets. `lichess/` is the multi-host,
  multi-module one, with a `run.sh` that drives the whole flow; `fishnet-farm/`,
  `limesurvey/`, `registry/`, and `website/` are smaller.

## Layout

```
apps/emet        the Emet compiler; builds the `emetc` binary
apps/emet-lsp    the language server, served from the compiler's own inference
apps/golemd      the per-host agent
apps/golemctl    the operator CLI: plan, apply, state, history, fleet
apps/fleet       a python harness that boots Debian VMs and deploys golemd to them
libs/scroll-format  the manifest and scroll model shared by writer and reader
lib              Emet libraries that ship with the toolchain (Quadlet, Traefik, …)
```

## CI

The whole test-and-build gate is `nix flake check` (`flake.nix`). It builds all
four binaries — `emetc`, `emet-lsp`, `golemd`, `golemctl`, each static-musl, so
one file runs on a Debian guest and on NixOS — and runs the Cargo workspace
tests, the `apps/fleet` harness tests, and the release-guard tests. The docs
site is in the gate too: that it builds, that the real nginx and the shipped
config actually serve the built pages, and that the published image assembles.
Any machine with nix runs the entire gate with that one command.

GitHub Actions runs it on every push to `main` and every pull request
(`.github/workflows/ci.yml`) and pushes what it built to the `dull-ca` cachix
cache, so other machines substitute instead of rebuilding. That arrangement is
interim: ADR 0035 puts the gate on a self-hosted box golem provisions itself,
which does not exist yet ([docs/design/ci-cachix-nix.md](docs/design/ci-cachix-nix.md)).

A `v*` tag runs `.github/workflows/release.yml`, which re-checks the guards,
runs the gate again — a tag can point at any commit — builds the docs image
(`nix build .#website-container`), and publishes it to ghcr.io (ADR 0050). The
intended way to make such a tag is `ci/release.sh`, which runs the same guards
before pushing it (ADR 0053).

A plain `nix build` is the developer-facing half of the same graph: it produces
`golem-tools`, all four binaries under `./result/bin`, installable outside the
checkout with `nix profile install <checkout>#golem-tools` ([QUICKSTART.md](QUICKSTART.md)).
