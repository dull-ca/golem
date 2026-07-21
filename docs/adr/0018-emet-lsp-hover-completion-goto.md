# 0018-emet-lsp-hover-completion-goto

## Status

Proposed (for review — not yet implemented).

## Context

`emet-lsp` (`apps/emet-lsp/`) is diagnostics-only. Its whole surface is
`diagnostics_for(source: &str) -> Vec<Diagnostic>`: on every `didOpen` /
`didChange` it hands the full document text to `emet::compile`, maps the first
`Error`'s byte span to an LSP range, and publishes. It holds no state, keeps no
document store, and calls the single-file `compile` — never `compile_file` — so
it never touches the import graph on disk.

`docs/TODO.md` §A wants the next tier of editor support: **hover** (the inferred
type under the cursor), **completion** (the in-scope names, with their types, at
a position), and **go-to-definition** (the defining site of the name under the
cursor, including across module files). The TODO already names the blocker: all
three "need the compiler to expose position-indexed type/scope information the
LSP can query." Today the compiler exposes none.

What the compiler retains, and what it throws away:

- **Spans exist everywhere.** `ast.rs` carries `Spanned<T>(T, Span)` on every
  expression, pattern, and arm, and `Decl` / `Variant` / `TypeDecl` / `Import`
  each carry their own `.span`. The parser threads byte ranges onto every node.
- **Inference sees the type at every span but discards it.** `infer_expr(inf,
  env, e: &Spanned<Expr>)` computes a `Type` for each node and has `e.1` (the
  span) in hand at the moment it does so — but it only ever *returns* the type
  upward or *unifies* it. Nothing records "the node at span S resolved to type
  T." `check_module` returns `(TyEnv, Type)` — the final env and `main`'s type —
  and drops the `Infer` (its `subst` / `row_subst`) entirely.
- **The environment at each point is transient.** `infer_expr` receives the
  `TyEnv` in scope at every node (lambda params via `env.insert`, `let` via
  `infer_decls`, `case` arms via `infer_pattern`), which is exactly the set
  completion needs — but that env is a call-stack value, never captured against a
  position.
- **Definition sites are known but not indexed.** A top-level `Decl` knows its
  own name and span; `resolve.rs` knows file path = module name and which names
  each module exposes (`Interface`), and `Import` carries its span. But no map
  from a *use* (a `Var`/`Ctor`/qualified-name occurrence, or a type name in a
  signature) back to its *definition span + file* is ever built.
- **`compile` throws away everything but errors and the final IR.** Even the
  substitution needed to resolve a `Type::Var` to a concrete type is gone by the
  time `compile` returns, because it lives on the dropped `Infer`.

The forces:

- **Elm's tooling is the muse.** Elm's language support treats the *compiler as
  the oracle*: the editor does not re-implement name resolution or type
  inference; it asks the compiler, which already did that work, for the answer
  at a position. `elm-language-server` orchestrates, but the type/scope truth
  comes from the compiler. Emet's `CLAUDE.md` already mandates Elm as the design
  reference for every decision — the LSP should follow suit rather than grow a
  second, drifting inference engine inside `emet-lsp`.
- **One inference engine, not two.** The one thing an LSP must never do is
  compute types differently from the compiler. Hover that disagrees with a
  diagnostic is worse than no hover. Whatever produces the answers must *be* the
  compiler's inference, not a parallel copy.
- **`emet-lsp` should stay thin.** Its value is LSP protocol plumbing
  (`lsp-server` wiring, span↔`Position` conversion, a document store),
  not language semantics. Emet's small-footprint invariant argues against a
  large new subsystem living in the editor crate.
- **Latency is per-keystroke.** Answers must be cheap enough to recompute as the
  user types, but the honest first cut can lean on whole-module recompilation
  rather than incremental re-inference.
- **Cross-file is real but bounded.** Go-to-definition across modules needs the
  import graph, which only `compile_file` (`resolve.rs`) walks. The LSP must move
  from single-string `compile` to path-aware, graph-aware compilation to answer
  cross-module queries at all.

## Decision

Adopt the **compiler-as-oracle** boundary. The inference engine in `emet` — the
one that already type-checks — additionally emits, alongside its result, a
**position-indexed store** describing what it inferred and what was in scope
where. `emet-lsp` stays a thin adapter: it owns a document store and the LSP
protocol, drives the compiler, and answers `hover` / `completion` /
`definition` by *querying that store*, never by re-deriving types or scopes.

The domain (language semantics) stays in `emet`; the I/O and protocol
(stdio, document lifecycle, `Position` math) stay in `emet-lsp`. Dependencies
point one way: `emet-lsp` → `emet`. This is the ADR's load-bearing structural
choice — everything below serves it.

### 1. What the compiler must start retaining: a `QueryIndex`

Inference gains an optional recording mode. When enabled, `infer_expr` /
`infer_pattern` record, at each node they visit, an entry into a new
`QueryIndex` built as a by-product of the pass that already runs. Concretely,
three tables keyed by span:

- **`types: Vec<(Span, Type)>`** — for each expression/pattern node, its
  inferred type. The type is stored **already `apply`-ed through the final
  substitution** (see §5 on when), so a stored `Type` is concrete/`Display`-able
  and carries no dangling `Type::Var` into the LSP. This is the hover source.
- **`scopes: Vec<(Span, ScopeId)>` + a `ScopeId → Vec<(Name, Scheme)>` table**
  — for each node, which lexical scope encloses it, and for each scope the names
  visible in it with their schemes. Built from the same `TyEnv` values
  `infer_expr` already receives; a scope is snapshotted whenever the env is
  extended (lambda, `let`, `case` arm) rather than per-node, so the store is
  proportional to binder count, not node count. This is the completion source.
- **`defs: Vec<(Span, DefSite)>`** — for each *use* occurrence (a `Var`, `Ctor`,
  qualified `Mod.name`, or a type-name mention in a signature), the span and
  module of its definition. Local binders resolve within the module; qualified
  and imported names resolve through the import graph (§3). This is the
  go-to-definition source.

`QueryIndex` is a plain data value with no behavior beyond lookup-by-position
(smallest enclosing span wins). It is the *only* new thing the compiler must
retain — and it costs nothing when recording is off.

Recording is gated so the normal `compile` / `compile_file` / `emetc` build path
pays zero cost: inference takes a flag (or the recorder is an `Option<&mut
QueryIndex>` threaded like the existing `inf` state), and the CLI never turns it
on. Only the LSP-facing entry point does.

### 2. Where it lives: a queryable API in `emet`, not logic in `emet-lsp`

Add an LSP-facing surface to `emet` (behind an `lsp` module or feature) that
returns the index next to the diagnostics:

```
pub struct Analysis {
    pub diagnostics: Vec<Error>,   // multiple, not just the first (see Consequences)
    pub index: QueryIndex,
}

pub fn analyze_source(src: &str) -> Analysis;            // single file
pub fn analyze_project(entry: &Path) -> ProjectAnalysis; // import graph
```

The query *operations* — "type at byte offset", "names in scope at offset",
"definition site of the use at offset" — are methods on `QueryIndex` in `emet`,
because they are language semantics (smallest-enclosing-span, scheme
instantiation for display, cross-module definition lookup). `emet-lsp` only
converts LSP `Position` ↔ byte offset (it already has `position_at` /
`span_to_range`) and marshals the results into `Hover` / `CompletionItem` /
`Location`. No inference, no scope reasoning, no import-graph walking lives in
`emet-lsp`.

This keeps the stable thing (the language) free of the volatile thing (the LSP
protocol and editor quirks), per the architecture doctrine: `emet-lsp` depends
on `emet`, never the reverse, and the semantics have exactly one home.

### 3. Cross-file: reuse the import graph, index every module

Go-to-definition across modules and correct hover/completion in a multi-file
project require the resolve stage. The LSP moves to a project-aware model:

- **`analyze_project(entry)`** runs the same load-order-check pass as
  `resolve::compile_entry`, but in *recording* mode, producing a per-module
  `QueryIndex` and keeping each module's `Interface`. The `Interface` is
  **extended to carry each exposed name's definition span and owning module**
  (its `Decl.span` / `Variant.span` / `TypeDecl.span`), which it does not today —
  this is the one addition `resolve.rs` needs so a *use* in module B can resolve
  to a *definition* in module A's file.
- A qualified use `Foo.bar` or an imported `bar` resolves through the extended
  interface to `(module Foo, span of bar's Decl)`, which the LSP turns into a
  `Location` with the file URI (path = module name, the rule `resolve.rs`
  already encodes) and the range.
- The LSP determines the entry/root by walking up from the open file (or a
  configured root); a file opened outside any import graph falls back to
  single-file `analyze_source`, which answers everything except cross-module
  go-to-def.

No new resolution path: this rides the exact dotted-name + import-graph
machinery ADR 0006 and ADR 0016 already built. The compiler stays the oracle for
cross-file just as it does within a file.

### 4. `emet-lsp` becomes a stateful document server

`emet-lsp` grows a **document store** (`Uri → text`, already implied by
`didChange`) and advertises the new capabilities (`hoverProvider`,
`completionProvider`, `definitionProvider`) in `ServerCapabilities`. On a
`textDocument/hover|completion|definition` request it: (a) looks up the current
text, (b) calls the `emet` analysis entry point (project-aware when the file is
in a graph, single-file otherwise), (c) converts the `Position` to a byte
offset, (d) queries the `QueryIndex`, and (e) marshals the result. It remains a
translator between the editor and the compiler-oracle — no language logic
crosses into it.

### 5. Incrementality: recompute-per-request first, cache later

The first cut is deliberately **non-incremental**: each hover/completion/def
request recompiles the relevant module (single-file) or project (multi-file)
from scratch with recording on, builds the `QueryIndex`, answers, and discards
it. This matches what the LSP does *today* for diagnostics (full recompile per
keystroke) and is the honest, minimal boundary. Emet modules are small; whole-
module inference is milliseconds.

Two cheap, non-architectural optimizations are permitted without changing the
boundary, and are called out as *later* work, not first-cut:

- **Cache the last `QueryIndex` per document version**, so the three requests
  that typically follow one edit (hover, then completion, then def) reuse one
  compile.
- **Reuse unchanged modules' interfaces** in `analyze_project` when only one file
  changed, since `resolve.rs` already processes modules in dependency order.

True incremental/query-driven re-inference (Salsa-style demand-driven
recomputation) is explicitly out of scope for this ADR. It is a large change to
`infer.rs`'s structure and buys latency the current module sizes do not need. If
projects grow enough to feel per-keystroke recompile, it gets its own ADR.

### 6. Spans on inferred types — the concrete compiler obligation

The single most important thing the compiler must *start* doing is **associate a
concrete (substitution-applied) `Type` with the span of the node it belongs
to**, at every node, when recording. Today the association is implicit and
transient (the type is a return value of `infer_expr`, the span is `e.1`, and the
two are never joined and stored). The change is to *join and store* them into
`QueryIndex.types` — which also forces a decision the compiler doesn't make
today: **when to apply the final substitution.** Because a node's type may
contain unification variables not yet resolved when the node is visited, the
recorded types are finalized in a second `apply` pass over the index after the
group/module solves (the substitution still lives on `Infer` at that point).
This is a mechanical pass, not new inference.

## Alternatives considered

1. **Re-implement inference/resolution inside `emet-lsp`.** Rejected outright.
   Two inference engines guarantee drift; hover would eventually disagree with
   diagnostics. It also duplicates the hardest code in the project (`infer.rs`)
   and violates the single-home / dependencies-point-to-stability doctrine. The
   whole point of the Elm model is that there is *one* oracle.

2. **Emit the index as a serialized side-artifact from `emetc` (e.g. a `.emeti`
   file the LSP reads).** Rejected for this tier. It reintroduces a stale-cache
   problem (the artifact lags the buffer the user is typing in), and the LSP
   needs the *current unsaved buffer's* types, not the last built file's. An
   in-process API call with the live text is simpler and always fresh. (A
   persisted index could later help cross-project indexing, but that is not what
   hover/completion/go-to-def in the open buffer needs.)

3. **A full query-driven / incremental compiler (Salsa-style) now.** Rejected as
   premature (YAGNI, Emet's small-footprint value). It is the right long-term
   answer to latency at scale, but current module sizes make per-request
   recompile fast enough, and it would be a far larger change to `infer.rs` than
   hover/completion/go-to-def warrant. Sequenced behind evidence of need, as its
   own ADR.

4. **Store raw (unsubstituted) types in the index and resolve lazily in the
   LSP.** Rejected: resolving a `Type::Var` needs the `subst`/`row_subst`, which
   are private to `Infer` and semantically the compiler's job. Handing a raw
   substitution to `emet-lsp` would leak inference internals across the boundary
   and put type-resolution logic in the wrong crate. The index stores
   already-`apply`-ed, `Display`-able types.

5. **Span-only go-to-definition (no cross-file); punt the import graph.**
   Rejected as too thin to be worth shipping: within-module def is easy, but the
   lichess-style multi-file libraries (ADR 0016) are exactly where jump-to-def
   earns its keep. The graph already exists in `resolve.rs`; extending its
   `Interface` with definition spans is a small, contained addition.

## Consequences

- **The compiler gains a recording mode and a `QueryIndex`.** `infer_expr` /
  `infer_pattern` learn to record `(span, type)`, `(span, scope)`, and
  `(use-span, def-site)` when a recorder is present, and a post-solve pass
  finalizes the recorded types through the substitution. Off (the default,
  `emetc` build path), the cost is zero. On, inference does one extra
  substitution pass and holds an index proportional to node + binder count.

- **The `emet` public surface grows** by an `analyze_source` / `analyze_project`
  pair returning `Analysis { diagnostics, index }` — position-queryable, and
  returning **multiple** diagnostics rather than only the first (the chumsky
  `Rich` multi-error recovery already noted in `docs/TODO.md` becomes worth
  wiring here, since an editor wants all errors, not one). `compile` /
  `compile_file` stay as they are for the CLI.

- **`resolve.rs`'s `Interface` must carry definition spans + owning module** for
  every exposed value, constructor, and type. This is the one change to the
  module stage; it forecloses treating an interface as purely a type/value env
  and makes it also a symbol table.

- **`emet-lsp` becomes stateful** (a document store) and advertises three new
  capabilities, but stays free of language semantics: it converts positions and
  marshals results, nothing more. Its dependency on `emet` deepens; the reverse
  dependency stays forbidden.

- **The LSP moves from single-string `compile` to path/graph-aware analysis** to
  answer cross-file queries — meaning it must learn a project root and file URIs,
  where today it only knows an opaque text blob.

- **Incrementality is deferred, on purpose.** First cut recompiles per request,
  matching today's diagnostics behavior; caching-per-version and unchanged-module
  reuse are named as follow-ups; Salsa-style demand-driven inference is out of
  scope and gated behind its own future ADR.

- **Cross-references:** rides the dotted-name resolution of ADR 0006 and the
  import graph of ADR 0016 (extending its `Interface`); preserves the single-
  `main` / `Scroll` bottom (ADR 0009) untouched — this ADR adds a read-only query
  surface, changing no language semantics or IR. It intersects the deferred
  multi-error parse recovery (`docs/TODO.md`, Diagnostics/tooling).
