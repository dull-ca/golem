# 0051 — Qualified constructors, and constructor identity that survives them

## Status

Accepted 2026-08-08. Supersedes ADR 0046, whose rejection rule it keeps in the
narrower form ADR 0049 gave ADR 0045's. Extends ADR 0016 (the module system) and
ADR 0049 (nested modules and qualified type identity). Touches no wire format:
an `emetc` front-end change, no `format_version` bump.

## Context

ADR 0046 recorded its own escape hatch as the thing that did not exist:

> The only way to make a shadowed constructor usable is to invent `CtorA.Wrap` —
> new grammar in expression *and* pattern position, a new resolution path
> through `infer_pattern`, and a rendering policy for diagnostics.

So it rejected instead. Importing two modules that each open-expose a `Wrap`
was a compile error at the second `import` line, because keying the constructor
registry by owner would have left both entries reachable by nothing: every
occurrence of a constructor in source carried exactly one piece of information,
its bare name, and `CtorA.Wrap` was a *parse* error in both positions.

Two things have changed since.

**ADR 0049 removed the harder half of the work.** It qualified type identity,
which is what makes two `Wrap`s distinguishable at all — before it, `Alpha` and
`Beta` were separate only by ADR 0045's rejection rule, and the constructor
question could not be asked cleanly. It also built the shape the constructor fix
needs: a pre-pass over a cloned module AST that rewrites source names to
`Owner.Bare` identities, an alias map from bare name to candidate identities, and
a de-qualifying policy for everything a reader sees. ADR 0049's own consequences
name this as the next step — "it becomes reachable to fix … but that is a
separate change."

**The grammar half turned out to be one parser combinator.** `Upper ('.'
Upper)*` with an adjacency test already existed twice — for dotted module names
and for ADR 0049's qualified type spelling. Constructors are the third caller of
the same rule, and the lowercase-tail test that already separates `List.map`
from a type name separates it from a constructor too.

What remains is that a constructor collision costs an author something ADR 0046
could only ask them to pay by renaming. Splitting a module by concern — the
operation ADR 0049 exists to make possible — is exactly what produces two
modules whose obvious constructor names agree.

## Decision

**A constructor is identified by its declaring module.** A variant declared in
`M` is `M.Ctor`. `resolve::qualify_module_constructors` rewrites a module's
variant declarations and every `Expr::Ctor` and `Pattern::Ctor` in it from what
the author wrote to the identity it means, before inference. Local constructors
are qualified along with imported ones — uniformly, so no stage downstream has
two kinds of constructor name to reason about.

**A constructor may be named by its qualified spelling, in expression position
and in pattern position.** `Shapes.Circle` builds and `Shapes.Circle` matches;
so does a parameter pattern, `f (Shapes.Circle s) = …`. The qualifier is what an
author can write in front of the dot — an import's name, its `as` alias, or this
module's own — and resolves to the module that owns the constructor. Nesting
follows ADR 0049: the split is at the last dot, so `Amb.Ctor.Hold` is `Hold` of
module `Amb.Ctor`.

**Bare stays the ordinary spelling.** A bare name with exactly one candidate in
scope resolves to it. Nothing in `lib/`, `examples/`, `apps/fleet/`, or
`sites/website/examples/` changes, and the qualified spelling is what an author
reaches for only when two constructors are in scope.

**ADR 0046's rejection survives at the reference, not at the import.** A *bare*
reference with two candidates has no single meaning and is an error naming both
modules and offering both qualified spellings — the same move ADR 0049 made on
ADR 0045's rule, and for the same reason: the ambiguity is real, but it is a
property of a use site rather than of an `import` line. Two imports that each
open-expose a `Wrap`, and a local variant sharing a name with an imported one,
now both compile as long as every mention says which.

**Prelude constructors and glyph match tags are not qualified and cannot be
shadowed.** They have no owning module, so they carry no identity and pass
through the rewrite untouched. `infer::register_type_decls` keeps refusing a
variant whose *bare* name is a prelude constructor's, which is what stops a
module from qualifying its way into owning `Just`.

**Evaluation reads the qualified module.** A `Value::Data` tag is compared
against the name in a `Pattern::Ctor`, so eval has to see the same names
inference did; running it on the module as written would compare `M.Ctor`
against `Ctor` and match nothing. Only the query index and LSP rendering read
the source AST, because those report source back to a reader.

**Every surface that shows a constructor to a reader de-qualifies it.**
Diagnostics print the bare tail (`infer::bare_identity`), and completion offers
the bare spelling wherever nothing else in scope shares it and the qualified one
where something does. A reader should not pay for the identity machinery in
every constructor they read.

## Consequences

- **Two modules may expose the same constructor name.** The programs ADR 0046
  rejected compile, provided each mention is qualified. That readmission is the
  point; the module split ADR 0049 enabled no longer stalls on the constructor
  namespace.
- **The ambiguity diagnostic moved and improved.** It now underlines the
  reference rather than an `import` line, and names an escape — both qualified
  spellings — instead of only telling the author to rename. Renaming is still
  available and is often still the better answer.
- **`Interface::ctor_owners`, `ConstructorOrigin` and
  `reject_constructor_name_collisions` are deleted**, along with both message
  builders. Identity carries what the rule used to guard, exactly as ADR 0049
  retired `reject_type_name_collisions`.
- **Preserved.** Every `.emet` tracked in the repo — 173 files — was compiled by
  the binary from this tree and from `origin/main`, comparing stdout bytes, exit
  status and rendered stderr. 170 are identical; the three that differ are the
  ADR 0046 negative fixtures, whose diagnostic deliberately moved from the
  `import` to the reference.
- **The parameter grammar widened.** `param_parser` now admits a qualified
  constructor, since the narrowness ADR 0044 needs is about which pattern
  *shapes* can appear in argument position, never about how a constructor is
  named.
- **A constructor identity now leaks into anything keyed by name.** The query
  index, `exposed_def_spans` and the completion table all hold `Owner.Ctor`, and
  each one needs a de-qualifying step at its edge. That is the same cost ADR 0049
  paid for types, and it is paid twice now.
- **Not adopted: making `M.Ctor` mandatory when two are in scope.** Elm rejects
  the ambiguous import outright and offers no qualified constructor at all; Emet
  now offers one and leaves the bare spelling working wherever it is
  unambiguous, which is the reading Haskell and Rust also take. The cost is that
  adding an open-exposed constructor to a library can make an importer's
  previously-fine bare reference ambiguous. That is a compile error naming the
  reference, never a silent change of meaning.
- **Not addressed: qualifying a constructor of a type reached through a
  re-export.** Constructors cannot be re-exposed (ADR 0046's observation still
  holds), so a constructor's owner is always the module that declared it and
  `M.Ctor` is spellable only where `M` is directly imported. Nothing here changes
  that, and no analogue of `inherited_type_owners` is needed.
