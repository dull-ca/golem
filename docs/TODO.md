# TODO / Backlog

Deferred work and known gaps, with pointers to the design/ADRs that explain the
context. Most entries are intentional deferrals. The few that are defects carry
a **KNOWN BUG** label and say what makes them wrong, so a deferral is never
mistaken for one.

Three backlogs: **A** is work inside the Emet language itself; **B** is the
Emet ↔ golem ecosystem integration, now that Emet lives in the golem
monorepo; **C** is CI and publishing. B's headline is model reconciliation.

## A. Emet language backlog

- **List patterns — DONE.** `case xs of [] -> … ; (x :: xs) -> …` now
  destructures a `List`: `::` is a right-associative level-5 operator (→ the
  `cons` builtin) and `[]` / `(head :: tail)` / `[a, b]` are patterns. This
  closes the self-recursion gap (ADR 0011) — recursion can now walk a list of
  hosts or glyphs. Exhaustiveness/redundancy covers lists by modeling `List` as
  a synthetic two-constructor (`[]`/`::`) sum through the existing Maranget
  checker (ADR 0005).

- **Mutual recursion for value decls — DONE.** Decls are grouped by dependency
  analysis into strongly-connected components (Tarjan) and each component is
  inferred together: every member is bound to a fresh monomorphic variable
  before any body is inferred, then the whole group is generalized once solved,
  so mutual recursion type-checks while a genuinely polymorphic group still
  generalizes and a monomorphic one stays monomorphic. Components are processed
  in dependency order, so forward references between non-recursive decls resolve
  and source order no longer matters. Evaluation ties the recursive knot per
  group (`RecGroup`), generalizing the former self-recursion binding to a set.

- **Full `appendable` `++` — DONE.** `++` is now `appendable -> appendable ->
  appendable`, satisfied by `String` and `List a` only, mirroring Elm exactly.
  `appendable` joins `number`/`comparable` as a third `Constraint` bounded type
  variable (ADR 0007), threaded through `bind`/`unify`/`merge_constraints`/
  `constraint_admits` in `infer.rs`; `merge_constraints` now rejects the
  unsatisfiable merges (`appendable` shares no type with `number`/`comparable`).
  `++` desugars to a single `append` prelude builtin carrying the `appendable`
  scheme, which dispatches at eval time to string or list concatenation on the
  runtime value. `List.append` / `List.concat` remain for explicit use;
  interpolation still desugars to `String.concat`.

- **One-line inline `case` — WON'T DO** (Dr. Dub, 2026-07-20). `case … of`
  requires laid-out arms; a single-line `case x of A -> a` inside a larger
  expression stays unsupported. Dropped as not worth the layout-close machinery.

- **User-facing `type` declarations — DONE.** Both non-parameterized user types
  (single-constructor record-carrying types, nullary sum types like `type Role =
  Web | Db`) and *parameterized* ones (`type Box a = Box a`, `type Result e a =
  Err e | Ok a`, recursive `type Tree a = Leaf | Node (Tree a) a (Tree a)`;
  design §6) parse, infer, drive `case` exhaustiveness, are usable in signatures,
  and cross module boundaries at arity > 0 via `exposing (T(..))`. Constructors
  generalize over the declared type parameters (`Box : ∀a. a -> Box a`).

- **Imported value constructors in scope for pattern-matching — DONE.** A
  type's constructors imported via `exposing (Type(..))` now resolve in
  `case x of Ctor -> …` on the importer side, and the exhaustiveness/redundancy
  checker sees the imported type's complete constructor set across the module
  boundary. The resolver carries each open-exposed type's constructor schemes
  and full variant set on the `Interface` and threads them into inference
  (`ImportedConstructors`), the pattern-side counterpart to the imported-type
  arities used for annotations. A type imported without `(..)` still keeps its
  constructors invisible to the importer (ADR 0016 §3).

- **Skolem-escape check for over-general signatures — DONE.** Signature checking
  no longer only instantiate-and-unifies (design §4.3 / ADR 0003): before the
  polymorphic body is generalized, each declaration's signature is *also*
  skolemized — every genuine signature type variable becomes a fresh, globally
  unique rigid constant — and the body is unified against that skolemized
  signature on a throwaway copy of the substitution. A signature the
  instantiate-and-unify pass accepts but the skolemized pass rejects claimed more
  polymorphism than the body delivers and is now a "signature is too general"
  type error, so `f : a -> a` over `x + 1`, a signature keeping two vars distinct
  the body forces equal, and a var the body forces to a concrete type are all
  rejected, while `id`/`const`/`map` and polymorphic recursion still pass. The
  three reserved bounded names (`number`/`comparable`/`appendable`) skolemize to
  a fresh constraint-carrying variable rather than a rigid, matching the bound
  they already ride through `bind`. Ordinary shape mismatches keep their original
  "type mismatch" message — the skolem check only relabels the strictly
  over-general case (`infer::check_signature_generality`). See ADR 0021.

- **Glyph pattern-matching — DONE (ADR 0017).** Glyphs and the filesystem
  `Entry` are matchable: a `case` destructures a built glyph by its PascalCase
  tag (`AptPackage`/`SystemdService`/`Filesystem`/`LineInFile`, and on the entry
  `File`/`Directory`/`Symlink`) while the reserved lowercase words still build.
  ADR 0002's symmetric injection was replaced by directed widening — a concrete
  glyph widens one-way into `Glyph`, never back — which is the soundness gain
  ADR 0008 named as its preferred route. Match-only constructors live in the
  pattern-resolution registries (`prelude::glyph_ctors`), and `eval` reifies a
  built glyph read-only for matching (`glyph_reified`/`entry_value`).

- **`emet-lsp` depth — DONE (ADR 0018).** `emet-lsp` (`apps/emet-lsp/`) now
  serves hover (inferred types), completion (names in scope), and go-to-definition
  (same-file and cross-file). Inference records position-indexed type, scope, and
  definition information into a `QueryIndex` (`query.rs`) when run with a
  `Recorder`, finalized by a post-solve `apply` pass so hover shows resolved
  types; the recorder is optional, so `compile`/`emetc` pay no cost. The adapter
  holds no language semantics — every answer comes from the one inference engine
  via `analyze_source`/`analyze_project`.

- **`Char` primitive + Elm-faithful `String`/`Char` modules — Accepted,
  implemented (ADR 0025).** A `Char` base type (one Unicode scalar; char literals
  `'c'` with `'\n'`/`'\t'`/`'\\'`/`'\''`/`'\u{...}'` escapes; comparable/orderable
  like Elm) plus the full `elm/core` `Char` and `String` surface — 12 `Char.*`
  and 35 new `String.*` builtins
  (`toList`/`fromList`/`map`/`filter`/`foldl`/`foldr`/`split`/`slice`/`contains`/
  `indexes`/`pad*`/ the `Char.is*` predicates, …), all scalar-indexed to match
  `String.length`. Skips locale variants (`toLocaleUpper`/`Lower`). Tests green
  (`apps/emet/tests/char_and_string.rs`, one Elm-parity assertion per function).

- **Tuple type + `String.uncons` — DONE (ADR 0027).** Emet now has a product
  type: tuples `(a, b)`/`(a, b, c)` (2–3 elements, 4+ redirected to a record at
  parse time) and unit `()`, in expression, pattern, and type position. Tuples
  are structurally `comparable` — comparable iff their elements are, compared
  lexicographically — and authoring-time only (never on the wire, like `Char`).
  The `Tuple` module (`pair`/`first`/`second`/`mapFirst`/`mapSecond`/`mapBoth`)
  and `String.uncons : String -> Maybe (Char, String)` ship with their exact Elm
  signatures, closing the ADR 0025 §4 deferral. Tests green
  (`apps/emet/tests/tuples.rs`).

- **Int + Char literal patterns; Float patterns rejected — DONE (ADR 0026).**
  `case` now matches `Int` and `Char` literals (typed `number`/`Char`, negatives
  allowed via a parse-time fold), riding the same open-domain exhaustiveness path
  as string patterns — a literal `case` needs a trailing `_`, duplicate literals
  are redundant. `Float` literal patterns are a compile error, Elm-faithfully
  redirecting to `<`/`>` comparison. Tests green
  (`apps/emet/tests/literal_patterns.rs`).

- **`let … in` inside a `case` arm is rejected, not supported — KNOWN GAP
  (AUDIT #26).** A `case` arm body cannot open its own `let` block; the arm's
  layout closes it before a laid-out `in`. The arm parser now names the form
  with a specific parse error (`let … in inside a case arm is not yet supported
  here — lift the binding out of the arm`) instead of letting it mis-parse into
  a misleading downstream type error (ADR 0032). Bindings a single arm wants
  must be hoisted above the `case`, or the arm must call out to a helper
  function that carries the `let`. Surfaced building `imageRef`
  (`lib/Quadlet.emet`), whose digest-vs-tag split is a top-level `if` and whose
  two arms are the separate `imageRefDigest` / `imageRefTagged` functions for
  exactly this reason. Future work: let a `case` arm body be a laid-out
  `let … in` like any other expression.

- **A negative literal as a function argument parses as subtraction — KNOWN
  GAP (AUDIT #27).** `f x -1` reads as `f x - 1` (binary subtraction), not
  `f x (-1)`; a negative literal in argument position needs parentheses
  (`f x (-1)`) or a named sentinel. It still surfaces only as a downstream type
  error, not a targeted parse-time rejection: distinguishing the negative-literal
  argument from binary `-` needs token-adjacency the lexer does not preserve, and
  a rejection that did not disturb subtraction was judged infeasible under ADR
  0032, so it is left deferred (the current subtraction parse is pinned by
  `negative_literal_argument_still_parses_as_subtraction` in
  `apps/emet/tests/diagnostics.rs`). This is the expression-argument residue of
  the unary-minus ambiguity — the *pattern* side was resolved in ADR 0026
  (negative literal patterns fold at parse time), but argument position still
  needs disambiguating. Future work: resolve `-1` as a negative literal in
  argument position too.

- **Record update and constructor patterns in argument position — DONE (ADR
  0044).** `{ r | field = value, … }` copies a record with the named fields
  replaced, over ADR 0010's rows: the base unifies with an open record demanding
  those fields, so a setter written against a row-polymorphic parameter serves
  every record shape carrying them. The rule is type-preserving — a new value
  must have the field's existing type and the result type is the base's — so an
  update never reshapes a record. A function or lambda parameter may also be a
  constructor pattern (`unwrap (Box held) = held`), restricted to
  single-constructor types so ADR 0005's exhaustiveness is never bypassed;
  `infer::reject_refutable_param` is the gate. A nullary constructor needs its
  parens (`f (Unit) = …`), where Elm accepts the bare form. Tests green
  (`apps/emet/tests/record_update.rs`, `apps/emet/tests/pattern_arguments.rs`).

- **Only a binder or `( Upper binder* )` may be a parameter — DEFERRED (ADR
  0044).** `parser::param_parser` is the whole parameter grammar, and it is
  narrower than the refutability rule requires. Three irrefutable forms Elm
  admits are parse errors: tuple parameters (`f (a, b) = …`), unit
  (`f () = …`), and nesting (`f (Wrap (Box s)) = …`). The gate does not object
  to any of them — `infer::reject_refutable_param` accepts `Pattern::Tuple` as
  a single-shape product (ADR 0027) and recurses through nesting — so widening
  the grammar to reach them needs no change to the type-level check. The
  narrowness was taken deliberately, because it is what keeps the refutable
  spellings (`f []`, `f "x"`, `f 0`) unwritable, and relaxing it must not
  reopen those.

  Bundled with it, and the more visible half: an excluded form reports a
  general parse error that does not name argument position as the limitation.
  `f (a, b) = a ++ b` says `found 'a' expected an expression`; a nested
  `f (Wrap (Box s))` says `found '(' expected an expression or ')'`. Both point
  at roughly the right place and describe the wrong repair, which ADR 0032 does
  not accept as a resting state. A `validate`-based redirect in `param_parser`
  would name the form; it was left out of ADR 0044's implementation as scope
  creep.

- **Duplicate fields in a record literal or update silently take the last
  value — DEFERRED.** `{ a = 1, a = 2 }` evaluates to `{ a = 2 }` and
  `{ r | a = 1, a = 2 }` applies `2`, where Elm rejects both. The literal path
  overwrites through `BTreeMap::insert` and the update path through the same
  insert into the copied map, so the two spellings are consistent with each
  other but not with Elm. Deferred rather than fixed alongside ADR 0044's
  record update precisely because a fix must cover both spellings together —
  fixing only the update would leave the older, more common literal silently
  wrong, which is the worse of the two states.

- **A row-polymorphic record type cannot be written in a signature — KNOWN
  GAP (ADR 0010).** The type parser builds `Row::Closed` records only, so
  `{ name : String | r }` is a parse error (`found '|' expected a type, '->',
  ',', or '}'`). Open record types exist and are inferred — that is the whole
  of ADR 0010 — but are unannotatable, so the headline shape ADR 0044's record
  update produces, a setter serving every record carrying a field, is exactly
  the shape whose type cannot be written down. Two consequences: such a
  function must be left unannotated to stay polymorphic, and `render_type`
  prints the open row as `{ prt : Int | .. }`, a spelling the parser cannot
  read back — so LSP hover shows a type that cannot be pasted into a signature.
  The fix is row-variable syntax in type position, on both sides.

- **Same-named types from two imports — FIXED (ADR 0045, then ADR 0049).**
  Reproduction
  showed the reported constructor-registry merge was the lesser half. Because
  type identity is a bare `String` in `Type::Con`, two modules' `Thing` were
  literally one type: a function typed `A.Thing -> …` accepted a `B.Thing`,
  carrying a `String` into a slot the compiler had proved `Int`. That fired
  with neither module exposing its type (private types reachable only through
  exposed *signatures*), and with a single import against a local declaration —
  neither of which touches `import_constructors` at all. The `case`
  (`unreachable!("non-exhaustive case")`) and argument-pattern
  (`unreachable!("refutable argument pattern survived inference")`) panics were
  downstream reports of the broken identity; the `case` route predates ADR 0044.

  ADR 0045 rejected the collision at the `import`, one owner per type name per
  module. **ADR 0049 then delivered the end state that ADR deferred:** a type
  declared in `M` is `M.Name`, `resolve::qualify_module_types` rewrites source to
  identities before inference, `M.Name` is a spelling an annotation may use, and
  only a **bare reference with two candidates** is still an error. A type reached
  through two imports is still one type; a privately-held name never mentioned in
  an exposed signature is still free.

- **Two imports may expose the same constructor name — FIXED (ADR 0046, then
  ADR 0051).**
  `resolve::import_constructors` keyed `ctor_schemes` by *bare constructor* name
  and `import_ty_env` bound `exposed_constructors` the same way, so with
  `CtorA.Alpha = Wrap String` and `CtorB.Beta = Wrap Int` imported together the
  last import won and `CtorA`'s `Wrap` became silently unreachable — `a : Alpha`
  / `a = Wrap "text"` reported `expected Int, found String`, naming the wrong
  type. Reproduction added two more shapes: the pattern side shadowed
  identically (`case v of Wrap s -> s` in a function annotated `Alpha -> String`
  reported `expected Beta, found Alpha` against the signature line), and a local
  `type Local = Wrap Int` silently displaced an imported `Wrap`. Never
  unsoundness — ADR 0045 keeps `Alpha` and `Beta` distinct, so no value crossed
  — but it cost an author a constructor that vanished without a word and a type
  error pointing at a type the author never wrote, so correct code read as
  broken.

  ADR 0046 fixed it by rejecting the program at the second `import`: a
  constructor was reachable *only* by its bare name, `CtorA.Wrap` was a parse
  error, so an owner-keyed map would have had no use site to select from and the
  shadowed constructor was never usable in the first place.

  **ADR 0051 supersedes that.** `Owner.Ctor` is now both the identity and a
  spelling an author may write, in expression and pattern position;
  `resolve::qualify_module_constructors` rewrites every constructor reference to
  its identity through `resolve::ConstructorScope`, and `import_constructors` /
  `import_ty_env` / `import_value_env` key by identity, so nothing displaces
  anything. Importing two modules that each open-expose a `Wrap` compiles as long
  as every mention says which. What is still rejected is a **bare reference with
  two candidates**, reported at the reference with both spellings offered —
  ADR 0049's narrowing of ADR 0045, applied to the constructor namespace.

### Diagnostics / tooling

- **Multi-error parse recovery + rich CLI rendering — DONE (ADR 0022).** The
  chumsky parser recovers past a bad declaration at the layout `;` boundary
  (`skip_until` at the top-level `item`, syncing on `Tok::VSemi`) and so collects
  several `Rich` errors in one run. A list-carrying surface threads them through:
  `parse_source_multi` / `compile_all` / `compile_file_all` return `Vec<Error>`
  (the resolve stage parses each module through the recovering path), while
  `parse_source` / `compile` / `compile_file` stay as first-error wrappers so
  existing callers are unchanged. `analyze_source` / `analyze_project` already
  returned `Vec<Error>`, so the LSP now surfaces every parse error. `main.rs`
  renders all diagnostics via ariadne (one report each). Type/eval/analyze stay
  first-error — inference is sequential — so a `Vec<Error>` holds either several
  parse errors or one later-phase error; the boundary is recorded in ADR 0022.
  Recovery is scoped to top-level (and does not touch the `let`-block
  `decls_parser`) to keep the ADR 0001 `parse-error(t)` / close-on-`in`
  handshake intact.

- **Missing `of` after `case` (#13) still reports a generic parse error —
  DEFERRED (ADR 0032 §4).** `case x` with no `of` still surfaces as `found
  'case' expected an expression, '(', '[', or '{'`, with no mention of the
  missing `of`. A targeted detection was scoped for this in ADR 0032 §2(f) but
  needs the general parse-error-recovery groundwork also deferred there;
  tracked as a follow-up rather than pinned in
  `apps/emet/tests/diagnostics_corpus.rs`.

- **Missing `in` after `let` (#25) still reports a generic parse error —
  DEFERRED (ADR 0032 §4).** `let x = 1` with no `in` still surfaces as `found
  'let' expected an expression, '(', '[', or '{'`, with no mention of the
  missing `in`. Same follow-up as #13, part of ADR 0032 §2(f)'s general-recovery
  dependency.

- **Empty record field value (#29) still reports a generic parse error —
  DEFERRED (ADR 0032 §4).** `name = , glyphs = []` still surfaces as `found
  ',' expected an expression, '(', '[', or '{'`, with no mention that the
  field's value is missing. Same follow-up as #13/#25.

- **`EvalError` is stack-passed, not boxed — KNOWN INTERIM.** Adding `span:
  Span` to `EvalError` (ADR 0032 §3) widened every frame across the giant
  `eval` match enough to require raising `EVAL_STACK_SIZE` to 1GB (from 512MB)
  to keep `RECURSION_LIMIT` firing before the native stack overflows
  (`eval.rs`). The root fix is boxing the error (`Result<Value, Box<EvalError>>`)
  to keep frames pointer-sized, at the cost of widening `run_module` /
  `eval_entry` / `eval_library` and their callers; the 1GB stack is an accepted
  interim rather than the long-term shape.

- **Cross-module conflicting-key spans degrade to the first glyph — KNOWN GAP.**
  `analyze` (`lib.rs`) locates a glyph-key conflict via `glyph_spans`, which is
  populated at eval time; when the second conflicting glyph comes from a
  library's eval-time value (not a literal glyph expression in the leaf being
  analyzed) its span is not captured, so the report degrades gracefully to the
  first glyph's span rather than 0..0. Precise dual-span reporting across a
  module boundary is a follow-up, not built here.

- **Did-you-mean has no forward references — KNOWN BOUNDARY.** The unbound-name
  suggestion in `infer.rs` draws candidates from `env.entries()` at the site of
  the reference, so a name defined later in the same file gets no suggestion.
  Deliberate for now; the fix is to seed candidates from all top-level
  declaration names rather than only already-inferred bindings.

### Cleanup

- **Remove the transitional `Str` / `Glyphs` aliases — DONE.** `String` and
  `List Glyph` are the only spellings; the parser no longer folds `Str` → `String`
  or `Glyphs` → `List Glyph` (ADR 0003). All examples/tests use the canonical
  spellings, and `Str`/`Glyphs` in a signature is now an ordinary "unknown type
  constructor" error.

- **Row polymorphism for record-parameter field access — DONE (design), see
  ADR 0010.** Field access needs a concrete record at the access site, so
  `\h -> h.name` and record-parameter helpers fail with "cannot infer record
  type for field access `.name`" (a record-typed signature does not rescue it
  either). ADR 0010 decides the fix: full row polymorphism (open/closed records,
  row unification), making `.name` row-polymorphic even without a signature (the
  Elm behaviour). The code change lands separately from the ADR.

## B. Emet ↔ golem integration backlog

Emet now lives in the golem monorepo (`apps/emet` and `apps/emet-lsp`). This
backlog is the work to make Emet golem's authoring language rather than a
standalone project.

- **Model reconciliation (headline) — DONE (ADR 0014).** The two vocabularies
  were reconciled by *retiring* golem's rich model rather than mapping onto it.
  golem's `Blueprint`/`State`/`Action` model was deleted; the compiled `Scroll`
  *is* the desired state. The shared model now lives in the `scroll-format`
  crate, and `golemd` diffs a manifest's scroll by content id and enacts the
  four glyphs. Emet's model — `main : List Scroll`, one `Scroll` per host, each
  a list of `aptPackage`/`systemdService`/`file`/`lineInFile` glyphs (ADR 0002 /
  ADR 0009) — is authoritative.

- **Binary wire format — DONE (ADR 0012/0013).** `emetc`'s default output is the
  content-addressed binary manifest (BLAKE3 over postcard, per-scroll and
  per-glyph content ids, `format_version`), defined in the shared `scroll-format`
  crate that both `emetc` (writer) and `golemd` (reader) depend on. `--text` and
  `--json` are the human/debug views.

- **golemd glyph rewrite — DONE (ADR 0014/0015).** `golemd` ingests the manifest,
  selects its host's scroll, diffs by content id (`GlyphOp` Install/Remove/
  Replace/Noop), and enacts through reversible `Reconciler`s (`apply`/`reverse`
  with journalled `Inverse`), collapsing revisions to `Init`/`Reconcile`.

- **Filesystem glyph: directories, symlinks, typed perms — DONE (ADR 0019).**
  The flat `file { path, contents, mode }` glyph generalized into one
  `Glyph::Filesystem { path, entry }` over a minimal-per-variant `Entry` sum
  (`File { contents, perms }` | `Directory { perms }` | `Symlink { target }`) with
  typed `Perms { mode: u16, owner, group }` — so illegal states (a symlink with a
  mode, a directory with contents) are unrepresentable. Emet gained `directory`
  and `symlink` reserved constructors alongside `file`; `mode` is now an octal
  parsed to `u16` in `emet` (a bad mode is a compile error). golemd's file
  reconciler became a filesystem reconciler that creates directories and symlinks,
  removing only the empty components it created (deepest-first, stopping at any
  non-empty or pre-existing one) and refusing to clobber a pre-existing entry.
  Unblocks host bind-mount source directories (the registry dogfood). A
  `format_version` bump 1→2.

- **Emet supersedes Nickel as the authoring language — DONE (ADR 0016).** Emet
  gained a minimal Elm-shaped module system (`module … exposing`, `import`), the
  lichess examples were re-authored in Emet (`examples/lichess/*.emet`), and the
  Nickel layer (`nickel/`, the `.ncl` examples, and golemctl's `nickel export`
  shell-out) was retired.

- **Quadlet workload library + shared module search path — DONE (ADR 0023 /
  ADR 0024).** A repo-root `emet.json` (`{ "source-directories": ["lib"] }`)
  gives every entry under the repo a shared library search path, so `import
  Quadlet` / `import Fleet` resolve to `lib/Quadlet.emet` / `lib/Fleet.emet`
  with no caller or flag change. The strongly-typed Podman-Quadlet library
  (`Image`/`Ref`, `Port`/`Proto`, `EnvVar`, `Restart`, `Mount`, `VolumeUnit`,
  `ContainerUnit`, the ergonomic `Workload` with `env`, and the
  `workloadGlyphs`/`containerUnitGlyphs`/`volumeUnitGlyphs`/`derivedVolumeUnits`
  lowerings) moved out of the test fixture into `lib/Quadlet.emet`. The registry
  and website dogfoods (`examples/registry/`, `examples/website/`) re-express on
  `Quadlet.Workload` instead of hand-rolled quadlet strings, and the lichess
  fleet (`examples/lichess/`) rebuilds its `workload`/`service`/`ingress`
  shortcuts as layer-c helpers over `Quadlet` (ADR 0023's three layers). No
  golemd/scroll-format change and no `format_version` bump — an Emet library
  above the four glyphs.

- **golem crates as flake outputs — DONE.** `flake.nix` exposes
  `golemd`/`golemctl` (plus static-musl `golemd-static`/`golemctl-static`),
  `emetc`, `emet-lsp`, and `website-container`; the
  static-build specifics are worked out (`pkgsStatic`) and CI (`.woodpecker.yml`)
  builds them.

- **Unify the docs sites — SUPERSEDED.** The two-tree split is the intentional,
  current decision, not a gap to close: the public Astro/Starlight site
  (`sites/website/`) carries the reader-facing docs, and the design record
  (`docs/` — ADRs, design, PLAN, TODO) stays internal and unpublished. The
  design docs are deliberately not folded into the site, and the site does not
  reference them — a "see ADR NNNN" on a public page would be a dangling pointer
  to something readers can't reach.

- **Retire the standalone `golem-lang` repo — DONE (2026-07-23).** golem builds
  and all tests pass with Emet embedded, so the standalone `golem-lang` source
  repo was archived (`~/personal-repos/golem-lang-archive-2026-07-23.tar.gz`)
  and its working copy removed.

- **Parallel apply (agreed next step, 2026-07-25).** `fleet apply` fans the
  same manifest out to every target host sequentially today. Across-host
  parallelism is the safe, fleet-side win: the hosts are independent machines,
  so applying to several at once is just concurrent HTTP with per-host reports —
  no golemd change. Within-host parallel *units* is the harder, deferred half:
  it would need golemd to serialize the shared, non-reentrant resources two
  units can contend on — apt/dpkg (one dpkg lock) and the apt package index —
  and to dedupe/queue per-file `lineInFile` writes so two units editing the same
  file don't race. So the plan is across-host parallelism first (fleet-side),
  with within-host parallelism gated on that serialization work (Dr. Dub's
  constraints, 2026-07-25).

- **Async apply.** `fleet apply` holds an HTTP request open for the whole
  reconcile; golemd should instead return 202 + a reconcile id and let the
  client poll for the report — replacing the hold-open request and
  prerequisite-shaped for parallel apply.

- **`run_reconcile_guarded`'s outer panic branch has no direct test.**
  `run_reconcile_guarded` (`foreman.rs`) wraps `run_reconcile` in its own
  `catch_unwind` for a panic anywhere in that call *besides* the reconciler
  (`PanicCatching` already catches a reconciler panic at the port, per its own
  doc comment). `a_panic_in_the_reconcile_is_contained_and_the_daemon_keeps_serving`
  (`tests/async_apply.rs`) only exercises the inner, `PanicCatching`-caught
  path; the outer guard's own recovery (the `Err(_)` arm — event push, then
  `recover()`) has no test that panics somewhere else in `run_reconcile` (e.g.
  `settle`) to trip it directly.

- **Rebuilt-report `FailPhase` should derive from `WalAction`, not a hardcoded
  `Enact`.** `projection.rs`'s cache-miss report rebuild (folding a settled
  attempt's WAL rows back into a `ReconcileReport`) always sets
  `GlyphFailure::phase` to `FailPhase::Enact`, regardless of the failing step's
  actual `WalAction` (`Apply`/`Reverse`/`Restart`). `FailPhase` also has
  `Reverse` and `Recovery` variants that a rebuilt report can never produce
  today. Deriving `phase` from the step's `action` would make a
  restart-recovery attempt's cache-miss report match the one served live from
  memory.

- **`reverse`/`diagnose` don't stream command output.** Only the apply path
  (`Reconciler::apply_streaming`) forwards host command output line by line to
  the progress ring (ADR 0033 §2); `reverse` and `diagnose` still run their
  `apt remove`/`systemctl status`/`journalctl` commands unstreamed, so a
  rollback shows lifecycle events but no live command lines, and forensics
  land only once captured in full. A `reverse_streaming` seam mirroring
  `apply_streaming` is the natural follow-up.

- **Convergence test for racing real applies (ADR 0034).** ADR 0034's bounded
  parallel-unit execution is live-verified on a real host, but there is no
  automated test that races two real, concurrent applies against the same host
  to confirm the per-kind locks (apt/dpkg, per-path `lineInFile`, the global
  systemd `daemon-reload`) actually serialize the contended resources and the
  WAL fold still converges. A follow-up, not covered by today's test suite.

- **Divergent-cid surfacing (ADR 0034 §1, Open questions).** Two units
  declaring the same `key` with different `cid` in one attempt silently
  last-wins on the host today, with nothing telling the author. ADR 0034
  records the wart but does not build the fix: a follow-up should make golemd
  detect and report the conflict at runtime (a warning event on the progress
  ring per ADR 0033 §2, plus a report note) and/or reject it at compile time in
  `emetc` (a cross-unit conflict check the analyze-time model does not have
  today).

- **Publishing.** The release-publishing mechanism is an open question — see
  §C and ADR 0035 §5 (ADR 0028's Forgejo/Codeberg channel died with the move to
  GitHub).

- **Dedup display semantics — OPEN DECISION (2026-07-27).** Under the parallel
  executor, shared-glyph duplicates usually race past the credit check and record
  honest idempotent observes, so they render `✓` rather than `≡` (only stragglers
  credit). Dr. Dub's options: (1) render by ownership (projection's shared/owner
  fields; cheap, but the displayed owner may not hold the real inverse), (2) status
  quo, (3) waiting-based dedup — non-first-declarers block on the owner's outcome
  (credit on success, real-apply on failure); deterministic credits, makes the
  waiting-dots UI real, un-pins the workers=1 count tests; requires an ADR 0034 §1
  revision (racing → waiting). Controller recommendation: (3).

## C. CI / publishing backlog

CI moved off Codeberg's Woodpecker to a self-hosted nix + cachix gate (ADR 0035;
`docs/design/ci-cachix-nix.md`). The gate (`nix flake check`) is live on
`lakin/ci-nix-cachix`; the automation around it is not yet stood up.

- **Stand up the self-hosted CI box, provisioned by golem (dogfood).** The
  poll-build-push loop in `docs/design/ci-cachix-nix.md` — golem's own four
  glyphs author the box that runs `nix flake check` on every push and pushes the
  closure to cachix. ADR 0035 §2. Until then an interim GitHub Actions workflow
  (`.github/workflows/ci.yml`) runs the same gate (ADR 0035 status amendment);
  retire it when the box is live.

- ~~**cachix activation.**~~ Done 2026-07-29: cache `dull-ca`, wired repo-scoped
  in `flake.nix` `nixConfig` and `devenv.nix`; the auth token lives in the
  `DULL_CA_CACHIX_PRIVATE_KEY` GitHub secret (and later on the CI box at mode
  `0600`). ADR 0035 §3.

- **Release publishing mechanism — OPEN (ADR 0035 §5).** ADR 0028's
  Forgejo/Codeberg channel is gone with the move to GitHub. The policy survives
  (tag-driven, one workspace version, static-musl artifacts, no crates.io); the
  channel is undecided — GitHub Releases pushed from the self-hosted box, or
  artifacts served from Dr. Dub's own infrastructure.

- **Sweep remaining `codeberg.org` references → GitHub.** In both repos: golem
  docs/site (`docs/guide/README.md`, `sites/website/astro.config.mjs`,
  `sites/website/src/grammars/README.md`,
  `sites/website/src/content/docs/getting-started/install.mdx`,
  `packaging/golemd.service`, `LAKIN-TODO.md`) and `emet.nvim`
  (`README.md`, `plugin/emet.lua`). Accepted ADRs keep their historical
  codeberg references — they are records, not live links.

## D. Dogfooding roadmap (2026-07-30)

The order of attack for putting golem in charge of Dr. Dub's real
infrastructure, on the bare-metal OVH box.

1. **Repeatable Debian wipe-and-reinstall on the OVH box.** Researched and
   documented: `docs/design/ovh-debian-reinstall.md` — one
   `POST /dedicated/server/{serviceName}/reinstall` call with inline
   partitioning, driven from a committed per-box JSON spec
   (`ovhcloud baremetal reinstall <server> --from-file … --wait`). Next
   concrete steps: create the OVH service-account credentials, write the
   box's spec file, do a first throwaway reinstall to burn down the doc's
   Unverified list (cloud-init user-data on stock Debian, which user gets
   the SSH key, real install duration).
2. **Host the personal static site (bacon.ca) via golem.** Currently on
   Vercel, built and deployed by a GitHub Action. Replace with: golem
   manifest authoring the web server + site content on the box; a build step
   producing the static assets. Retires the Vercel dependency and its CI.
3. **Move CI onto the box, golem-managed.** The §C CI-box item, now with a
   sharper shape: webhook-triggered (or polled) PR/push builds running
   `nix flake check` with cachix push, progress visible. Candidate runners
   to evaluate rather than hand-rolling everything: the poll-loop sketch in
   `docs/design/ci-cachix-nix.md` vs. an existing nix-friendly runner.
   Retires the interim GitHub Actions workflow.
4. **(Parked) Kubernetes.** Two distinct ideas, both explicitly deferred:
   golem regularly converting machines into k8s cluster nodes (Talos or
   similar), and a k8s-flavored authoring layer in Emet (e.g. a
   helm-template-with-parameters glyph, or library abstractions for
   pods/deployments/statefulsets). Bigger undertaking; not now.
