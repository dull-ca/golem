# 0019-filesystem-glyph

## Status

Accepted 2026-07-20; implementation to follow. Generalizes the `file` glyph
introduced in ADR 0002 and reconciled in ADR 0015, superseding their `file`
shape.

## Context

golemd owns exactly four glyphs (root `CLAUDE.md`): `aptPackage`,
`systemdService`, `file { path, contents, mode }`, and `lineInFile`. The
standing invariant is "there is no fifth resource kind" — richer shapes are
Emet library abstractions that *compile down* to these four, never new golemd
reconcilers (ADR 0002, ADR 0015 §Consequences).

The registry dogfood exposed a gap that no composition of the four can fill: a
host bind-mount volume needs its **mount source directory to exist first**.
Podman `statfs`-refuses a bind mount whose source is absent, so a scroll that
runs a container with `-v /srv/registry/data:/var/lib/registry` cannot succeed
unless golem has already ensured `/srv/registry/data` is a directory. Today golem
can only create a *file*; a directory is not expressible. A `file` glyph's
reconciler already `create_dir_all`s the file's **parent** directory
(`reconcilers::write_file_atomic`), so the machinery for making directories half
exists — but there is no way to declare "this path is a directory" as the desired
state, and nothing captures a directory in the reversal record.

Two further shortcomings surfaced alongside the directory gap:

- **`mode` is stringly typed.** `Glyph::File.mode` is a `String` like `"0644"`,
  parsed at reconcile time (`reconcilers::parse_mode`) and *only* at reconcile
  time — a malformed mode is a runtime `Fatal`, not a compile error. There is no
  owner or group at all: golem writes every file as whatever uid golemd runs as.
- **A directory and a symlink are as bottom-level as a file.** They are the same
  kind of thing — a filesystem entry at a path — differing in *what the entry is*
  and, crucially, in *which fields even mean anything*. A file has contents and
  permissions; a directory has permissions but no contents; a symlink has a
  target but neither contents nor a meaningful mode of its own (the target's
  perms govern on Linux). That is precisely an ADT: a sum over the entry kind
  where **each arm carries only the fields valid for it**, so a shared or
  optional field that is meaningless for some arm never exists to be misfilled.

The tension to resolve honestly: is a directory a **fifth glyph**, violating the
invariant, or is `file` itself the special case of a more general **filesystem
glyph** whose payload is a sum `File | Directory | Symlink`? If the latter, the
count stays four and the invariant holds — we are generalizing an existing
primitive, not adding one. Elm/ADT modeling is the muse: **make illegal states
unrepresentable** (Dr. Dub's review ask). Model the entry as a sum where every
arm carries exactly its own fields — `File` its `contents` and `perms`,
`Directory` its `perms`, `Symlink` its `target` — so no arm can hold a field
that is meaningless for it. `file`/`directory`/`symlink` become three spellings
of one glyph.

Constraints that bound any answer:

- **The wire format is non-self-describing** (ADR 0012/0013): a `Glyph`
  variant's field and variant order *is* the postcard encoding. Changing
  `File`'s fields — or adding `Directory`/`Symlink` variants — is a
  `format_version` bump (`FORMAT_VERSION`, currently `1`), not a free refactor.
- **Reversibility is a property of the (glyph, prior-host-state) pair** (ADR
  0015): every new kind needs an `Inverse` that restores exactly what apply
  changed, and golem must never remove a directory it did not create.
- **The IR is inert, concrete data** and every glyph field today is a plain
  `String` (ADR 0004). Structured permissions are the first non-`String`,
  non-flat glyph field — a deliberate departure to weigh.
- **The surface constructors are reserved words** special-cased in
  `parser.rs::is_reserved_constructor` / `build_constructor`, not ordinary
  records. Any new spelling is new parser special-casing plus new `ast::Expr`,
  `infer`, and `eval` arms.

## Decision

**Generalize `file` into one filesystem glyph whose payload is a minimal,
correct-by-construction entry sum `File | Directory | Symlink`, each arm carrying
only the fields valid for it. Keep the glyph count at four.** `Directory` and
`Symlink` are *variants of the entry*, not new glyphs — so the "four primitives"
invariant holds by construction, exactly as `AptPackage | SystemdService` are two
variants of one `Glyph` sum (ADR 0002), not two entries in a count of resources.
The entry sum is designed so that **illegal states are unrepresentable** (Dr.
Dub's review ask): there is no place to put contents on a symlink, or a mode on a
symlink, because those fields do not exist on that arm.

### 1. The wire model (`scroll-format`)

Replace `Glyph::File { path, contents, mode }` with a `Filesystem` glyph whose
payload is an Elm-style sum. `path` — the one field common to and meaningful for
every entry — lives on the glyph; everything else lives *inside the arm that
gives it meaning*:

```rust
pub enum Glyph {
    AptPackage { name: String },
    SystemdService { unit: String },
    Filesystem { path: String, entry: Entry },
    LineInFile { path: String, line: String },
}

/// What lives at `path`. Every arm carries ONLY the fields valid for it — no
/// shared or optional field that is meaningless for some variant. This is the
/// "make illegal states unrepresentable" discipline: a symlink cannot carry
/// contents or a mode because those fields are not on its arm.
pub enum Entry {
    File { contents: String, perms: Perms }, // inline contents + permissions
    Directory { perms: Perms },              // permissions, but no contents
    Symlink { target: String },              // a target; no contents, no perms
}

/// Permissions as typed data, not a stringly `mode`. `mode` is the 12
/// permission bits (setuid/setgid/sticky + rwxrwxrwx) as a `u16`; owner/group
/// are names resolved to uid/gid at reconcile time, `None` = leave as-is.
/// `Perms` appears only on the arms where it is meaningful (`File`, `Directory`).
pub struct Perms {
    pub mode: u16,               // 0o0000..=0o7777
    pub owner: Option<String>,
    pub group: Option<String>,
}
```

- **`entry` is the ADT, and each arm is minimal.** `contents` lives *only* on
  `File`; `target` lives *only* on `Symlink`; `perms` lives *only* on `File` and
  `Directory`. The shape reads "an entry, which is a file with these contents and
  perms / a directory with these perms / a symlink to this target" — never "an
  entry with optional contents/target/perms that only mean something for some
  kinds." Each datum sits on exactly the arm that carries it; the sum makes the
  meaningless combinations (a symlink's mode, a directory's contents) impossible
  to even write down. This is the classic reason to reach for a sum-of-minimal-
  records over one record-with-nullable-fields.
- **`Symlink` carries no `perms` at all.** A symlink's own mode is not meaningful
  on Linux — the target's permissions govern — so rather than a uniform `Perms`
  the symlink arm *skips* it entirely. The earlier sketch's "uniform `Perms` on
  every kind, symlink reconciler skips the `chmod`" is rejected precisely because
  it makes an illegal state representable (a symlink with a mode). Dropping the
  field is the correct-by-construction form: there is nothing to skip because
  there is nothing to set.
- **`mode` becomes a `u16`, not a `String`.** The 12 permission bits are exactly
  a bounded integer; `"0644"` was a `String` only because every glyph field was.
  A `u16` makes a malformed mode *unrepresentable in the IR* — the octal is
  parsed once, in `emetc`, at compile time. Rejected sub-alternative: a nested
  `{ setuid, setgid, sticky, user: {r,w,x}, … }` record — faithful but noisy on
  the wire and in every match; the bits are already a well-understood
  12-bit number, so a `u16` with named-constant helpers is the right altitude.
- **`owner`/`group` are `Option<String>` names**, resolved to uid/gid on the
  host at reconcile time (a name is portable across hosts in a way a raw uid is
  not). `None` means "do not manage ownership" — the honest default for the
  registry case, where the directory just needs to exist.
- **`key()` stays `file:<path>`** — one entry per path is one resource,
  regardless of kind, so the diff/versioning identity (ADR 0015 §2) is unchanged.
  Renaming the key namespace would be gratuitous churn. `describe()` gains a
  per-kind phrasing ("ensure directory `…`", "ensure symlink `…` -> `…`").

### 2. The Emet surface

Keep three ergonomic reserved constructors that **desugar to the one glyph** — the
surface stays close to today's `file { … }` while the IR unifies:

```
file      { path, contents, mode }          -- Entry::File   { contents, perms }
directory { path, mode }                    -- Entry::Directory { perms }
symlink   { path, target }                  -- Entry::Symlink { target }
```

- `file`, `directory`, `symlink` are all reserved lowercase constructors
  (`is_reserved_constructor`), each requiring exactly its fields
  (`build_constructor`), each building the single `Expr::Filesystem { path,
  entry }` with the right `entry` arm. **The per-arm field set is enforced at the
  surface**: `symlink` accepts no `mode` field (a symlink has no meaningful
  mode), and `directory` accepts no `contents` — so the "illegal states
  unrepresentable" property reaches all the way up to what an author can even
  write. This mirrors how `aptPackage` and `systemdService` are distinct
  spellings that both inject into `Glyph` (ADR 0002). The surface grows two
  words; the IR grows zero glyphs.
- **`mode` on the surface stays an octal literal** for authors (`0o755` /
  `"0755"`), lowered to the `u16` in `eval`. Owner/group are optional record
  fields defaulting to absent, so existing `file { path, contents, mode }`
  programs parse and lower unchanged — only their *target IR variant* differs.
- **Type surface — resolved.** The first-class glyph type `File` (ADR 0002, used
  in signatures like `webserver : String -> File`) is renamed/subsumed. This
  ADR's previously-open question — one opaque `Filesystem` type vs. `File` /
  `Directory` / `Symlink` sibling glyph types — is **settled in favor of one
  `Filesystem` type** that all three constructors produce. There is exactly one
  `Glyph::Filesystem` reconciler kind, so the "four reconciler kinds" framing
  holds unchanged; the entry sum lives *inside* that one kind as its payload,
  making the illegal per-arm combinations unrepresentable without splitting the
  glyph into three. Sibling glyph types are rejected: they would fracture one
  reconciler into three, multiply the parser/infer surface, and buy a "returns
  specifically a directory" precision no code needs, since nothing eliminates on
  the glyph type today (the ADR 0002/0008 injection is still elimination-free).
  The distinctions that *do* matter — contents vs. target, perms vs. no perms —
  are captured by the `Entry` sum, which is where correctness-by-construction
  belongs, not by a proliferation of top-level types.
- **Interaction with the just-landed parameterized `type`/ADT support (ADR
  0016).** It is tempting to let authors write `Entry` as an ordinary Emet
  `type` and pass it to a *single* `filesystem { path, entry }`
  constructor — the language now has the ADT machinery to express it. Rejected
  for the surface: the reserved glyph constructors are deliberately *not*
  ordinary records/types (they special-case field checking and injection), and
  coupling a golemd wire variant to a user-space `type` declaration would make
  the wire contract depend on library code — inverting the "language is
  untouched, capability lives in glyphs+reconcilers" invariant (ADR 0002
  Consequences). `Entry` lives in `scroll-format` as the wire ADT; the
  surface exposes it only through the three fixed constructors. The new ADT
  support *does* pay off one level up: the userland library abstractions that
  compile down to these glyphs (registry volume, service dir, …) are now much
  easier to model as real sum types.

### 3. The golemd reconciler (`reconcilers.rs`, ADR 0015 §4)

`apply`/`reverse` gain the two new arms; the existing `File` path is preserved
verbatim under `Entry::File { contents, perms }`.

- **`Directory { perms }` apply.** Observe the path. If it is already a directory
  with the desired perms → `changed = false`, `Inverse::Nothing`. If absent →
  `create_dir_all`, set perms/ownership, record a **new** inverse
  `Inverse::RemoveDirectory { path, created: <the deepest components golem
  actually created> }`. If it exists but perms differ → `chmod`/`chown`, record
  `Inverse::RestoreDirMeta { path, prior_perms }`.
- **`Symlink { target }` apply.** No `perms` to apply — the arm carries none, so
  there is no `chmod` step to write or skip. If the path is already a symlink to
  `target` → no-op. If absent → `symlink(target, path)`, record
  `Inverse::RemoveSymlink { path }`. If a different entry exists at the path →
  this is the one genuinely new hazard; recommend **refuse** (a
  `Fatal`/`Retryable` error) rather than clobber a pre-existing file, matching
  golem's "never touch state it did not create" stance (§4 below).
- **Permissions.** A shared `apply_perms(path, &Perms)` resolves owner/group
  names to uid/gid, `chown`s when set, and `chmod`s to `mode`. It is invoked
  *only* from the arms that carry a `Perms` — `File` and `Directory` — so there
  is no code path that could apply a mode to a symlink; the type forbids it.
  Reused by the `File` path too, which today only sets mode — so this *adds*
  owner/group to the existing file glyph as a free consequence of the
  generalization.

### 4. Reverse / Inverse for a directory (created-by-us vs pre-existing)

This is the subtle part, and it is the same discipline ADR 0015 already applies
to `lineInFile` and `aptPackage`: **golem reverses only what it created.**

- A directory golem **created** reverses by removal — but `create_dir_all` may
  have created *several* nested components (`/srv/registry/data` might have made
  `/srv/registry` and `/srv/registry/data`). The inverse records the ordered list
  of components golem actually created (deepest first); reverse `rmdir`s them
  deepest-first, and **stops at the first non-empty directory** (a later glyph or
  a container may have populated it). Never `rm -rf`: golem removes empty
  directories it made, nothing it did not.
- A directory that **pre-existed** records `Inverse::Nothing` (perms already
  matched) or `Inverse::RestoreDirMeta` (golem only changed perms/owner) — reverse
  restores the prior mode/owner/group and never removes the directory.
- A **symlink** golem created reverses by `unlink`; a pre-existing correct
  symlink records `Inverse::Nothing`.
- New `Inverse` variants: `RemoveDirectory { path, created: Vec<String> }`,
  `RestoreDirMeta { path, prior_perms: Perms }`, `RemoveSymlink { path }`. The
  existing `RestoreFile`/`DeleteFile` stay (now carrying `Perms`, so their
  postcard shape changes — part of the same `format_version` bump). This keeps
  the LIFO composite-reversal contract (ADR 0015 §3/§5) intact: a scroll that
  makes a directory then writes a file into it reverses file-then-directory,
  which is exactly the order that leaves the directory empty and removable.

### 5. Wire-format / `format_version` impact and migration

- Replacing `File`'s fields with the `Entry` sum (adding `Directory` and
  `Symlink` arms) plus the `Perms` struct is a **`format_version` bump**
  (`1 → 2`) by the ADR 0012/0013 rule: the
  postcard layout changes, and `check_format_version` will cleanly reject a v1
  manifest rather than misparse it. The `Inverse` enum in golemd's journal is
  *not* the manifest wire format, but it is a persisted, serialized type — its
  new variants and the `Perms` on `RestoreFile` change the journal's on-disk
  postcard too; existing journals must be handled (recommend: journals are
  golem's own memory, keyed to a golemd version, and the dogfood fleets are
  disposable — a clean-cut re-init is acceptable, versus writing a v1→v2 journal
  migration).
- **Migration is a clean cut, not a dual-read.** `format_version` exists exactly
  to make this a typed error at rest, not a silent misread. `emetc` and `golemd`
  upgrade in lockstep (they already must — ADR 0013 ships them from one crate).
  No v1 manifests are in production; the cost is a version bump and a
  recompile-and-reship, which is the anticipated evolution path, not an
  exceptional one.

### 6. Does the four-primitive invariant hold?

**Yes — and this ADR sharpens what "four primitives" means.** The invariant is
about the *count of reconciler-owned resource kinds golemd must know how to
enact*, not the count of surface spellings. `file`/`directory`/`symlink` are one
glyph with one reconciler and one `key()` namespace; `Entry` is an internal
sum exactly as `Glyph` itself is. golem gains a directory capability without a
fifth reconciler, which is the letter and the spirit of ADR 0002/0015. The root
`CLAUDE.md` should be updated to describe the primitive as a **filesystem glyph
(file / directory / symlink)** rather than a bare `file`, making explicit that
the entry kind is a sum, not a new count.

## Alternatives considered

1. **A bare `directory` fifth glyph** (`Glyph::Directory { path, mode }`).
   Rejected: it *is* the forbidden fifth reconciler kind, and it fractures the
   filesystem-entry concept across two glyphs — a directory and a file are the
   same primitive (an entry at a path with perms) differing only in kind, so two
   glyphs would duplicate the `key()` namespace, the parent-`mkdir` logic, and
   the perms handling. It also leaves `mode` stringly typed and adds no owner/group.
   The generalization gets directories *and* fixes permissions *and* keeps the
   count, for the same or less reconciler code.
2. **Do nothing; use named volumes instead of bind mounts.** A podman *named*
   volume needs no pre-existing host directory, sidestepping the `statfs` refusal
   entirely. Rejected as the primary answer: it dodges the modeling gap rather
   than closing it (golem still cannot ensure a directory, which recurs for any
   host-path need — sockets dirs, log dirs, config trees), and it constrains the
   Emet library authors' choices to work around a missing primitive. Worth noting
   as a *workaround* the dogfood can use today, but not the decision.
3. **Model directories purely in userland** (an Emet function that emits a
   `lineInFile`/`file` hack to force a directory). Rejected: there is no
   composition of the four current glyphs that creates an *empty* directory —
   `file` makes a file (and only mkdir's the parent as a side effect, uncaptured
   in any inverse). The capability genuinely does not exist below the glyph layer,
   so it must be added at the glyph layer. This is the ADR 0002 rule working as
   intended: a new *capability* is a new IR variant + reconciler; here the variant
   is an `Entry` case, not a new `Glyph`.
4. **Keep `mode: String`, add only the `Entry` sum.** Rejected as a half-measure:
   the wire format must break for the sum regardless, so the `format_version` bump
   is already paid — fixing the stringly permissions in the same break is nearly
   free and avoids a *second* bump later. Typed permissions are the other half of
   "model this properly."
5. **A user-space `type Entry` passed to one `filesystem` constructor** (lean
   on ADR 0016 ADTs). Rejected for the wire/surface coupling reason in §2: the
   glyph wire contract must not depend on a library `type` declaration. The ADT
   support pays off in the *abstractions above* the glyphs, not in the glyph
   definition itself.
6. **One flat `Filesystem { path, kind, perms }` with a shared `perms` on every
   entry** (an earlier draft of this ADR). Rejected on Dr. Dub's review: a shared
   `perms` is meaningless for a `Symlink` (its mode is not honored on Linux) and a
   shared `contents` would be meaningless for a `Directory` — a
   record-with-fields-that-only-sometimes-apply is exactly the illegal-states
   trap. The accepted model pushes `perms` and `contents` *into the arms that give
   them meaning* (`File`, `Directory` for `perms`; `File` for `contents`), leaving
   `path` — valid for all — as the only common field. **Make illegal states
   unrepresentable**: a symlink with a mode, or a directory with contents, cannot
   be constructed, serialized, or matched, because the fields do not exist on
   those arms.

## Consequences

- **golem can ensure directories and symlinks**, unblocking host bind-mount
  volumes (the registry dogfood) and any host-path need, without a fifth
  reconciler. The "four primitives" invariant holds — reframed as four
  *reconciler kinds*, with the single `Glyph::Filesystem` reconciler carrying an
  internal `Entry` sum as its payload.
- **Illegal filesystem states are unrepresentable** (Dr. Dub's review ask): the
  `Entry` sum's arms are minimal and correct-by-construction — `File { contents,
  perms }`, `Directory { perms }`, `Symlink { target }` — so `contents` exists
  only where a file has them, `perms` only where a mode is meaningful, and
  `target` only on a symlink. There is no shared or optional field that applies
  to some arms and not others; a symlink-with-a-mode or a directory-with-contents
  is not merely rejected at runtime, it cannot be written down at any layer
  (surface constructor, IR, wire, or `case`).
- **Permissions become typed data**: `mode` is a `u16` validated in `emetc` (a
  bad mode is now a compile error, not a reconcile-time `Fatal`), and
  owner/group arrive for *all* filesystem entries, including plain files —
  a capability the current `file` glyph lacks.
- **A `format_version` bump (`1 → 2`)** and a lockstep `emetc`/`golemd` reship;
  the persisted journal's `Inverse`/`Outcome` shape also changes, handled by a
  clean re-init of the disposable dogfood fleets rather than an on-disk migration.
  This is the anticipated evolution path (ADR 0012/0013), not an exceptional cost.
- **New reverse hazards are contained by the existing discipline**: golem removes
  only empty directories it created (deepest-first, stopping at any non-empty
  or pre-existing component) and refuses to clobber a pre-existing entry at a
  symlink's path — the ADR 0015 "never touch state it did not record creating"
  rule, extended to directories. `RemoveDirectory` must record the exact nested
  components created, or reverse will either leak or over-delete.
- **The surface grows two reserved words** (`directory`, `symlink`) and the
  parser/`infer`/`eval` gain the corresponding arms; `file`'s existing programs
  are source-compatible (same spelling, new IR target). The single first-class
  glyph type becomes `Filesystem`, subsuming `File` in signatures.
- **Forecloses** a future where directories are a distinct top-level resource
  with their own key namespace or reconciler; commits golem to the "filesystem
  entry is one primitive, `Entry` is a minimal-per-arm sum" model. A later need
  for a fourth entry kind (device node, fifo, hardlink) is then a new `Entry`
  arm carrying only *its* fields (another `format_version` bump), not a new glyph
  and not a widened shared record — the path this ADR establishes.
- **Cross-references:** generalizes the `file` glyph of ADR 0002 and its
  reconciler/`Inverse` from ADR 0015; a `Glyph` variant/field change is pinned by
  `format_version` per ADR 0012/0013; the userland abstractions that compile down
  to this glyph are authored in Emet per ADR 0016, whose new parameterized-ADT
  support this ADR leans on *above* the glyph layer but deliberately not *in* the
  wire contract.
