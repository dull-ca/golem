# Golem scaffold review

Reviewing the tarball under `files.zip` against the design commitments in `README.md`.
Findings ordered by severity, not by your numbered sections.

---

## 1. Showstopper: journal-before-mutate is misimplemented in all three providers

This is the design commitment that "must make 'crashed mid-apply, falsely thinks it
was preexisting' structurally impossible." It's broken in exactly that direction.

The discipline `state.rs:8-10` describes is correct. The reconciler does not implement it.

**Where the discipline breaks**, walking `crates/golemd/src/reconcile.rs:162-200`:

```rust
// Journal first-touch capture BEFORE mutation. Provider's
// `apply` writes preexisting + backup into `st`; ...
st.last_spec = Some(claim.spec.clone());
self.store.put(&st)?;                      // (A) tombstone

let prev_hash = st.content_hash.clone();
match apply_with_retries(provider.as_ref(), &claim.spec, &mut st).await {
    Ok(()) => {
        ...
        self.store.put(&st)?;              // (B) post-apply put
```

At (A) the tombstone has `preexisting: false, installed_by_us: false, backup: default`
because `st` is `ClaimState::fresh(id)` for a never-before-seen claim. The provider's
`apply` then does, in order:

1. Read reality (file / dpkg-query / systemctl show).
2. Mutate `state.preexisting` and `state.backup` **in memory only**.
3. Mutate the OS (`write_atomic`, `apt-get install`, `systemctl enable/start`).
4. Set `state.installed_by_us = true` in memory.
5. Return `Ok(())`.

The post-apply `put` at (B) is the **first** time first-touch state lands on disk.
Crash anywhere between step 3 and (B) leaves the journal in a worse-than-empty
state: it says "we never touched this" while reality contains our mutation.

### What goes wrong on recovery

Three concrete scenarios, one per provider. All assume SIGKILL after step 3 but
before (B), then a clean restart.

**`file.rs`** — first apply of `/etc/caddy/Caddyfile` to a path that already had a
user-authored file. Provider reads the user's file, sets `preexisting=true,
backup.prior_content=Some(<user content>)` in memory, calls `write_atomic` (our
content lands), KILL. On restart: journal still says `preexisting=false,
backup=default`. `observe()` returns Present, `matches()` returns true (we wrote
the spec content), fast path skips. Forever after, `state.preexisting=false`,
`state.backup` is empty. On orphan unapply (`file.rs:140-156`): both
`preexisting` and `installed_by_us` branches are false → returns Ok without
touching the file. The user's file is silently overwritten and never restored.
Data loss.

**`apt.rs`** — first apply of `caddy` on a host that had no caddy. Provider sees
preexisting=false, runs `apt-get install caddy`, KILL after install but before
returning. On restart: matches() returns true (caddy is installed), fast path
skips apply. Journal `installed_by_us=false`. On orphan unapply
(`apt.rs:139`): `state.preexisting || !state.installed_by_us` is `true` →
returns Ok without uninstalling. The package leaks permanently — Golem now
holds caddy on this box but its own journal believes "never installed by us, so
not ours to remove."

The inverse case is symmetric: caddy was already installed by a human, we set
`preexisting=true` in memory, KILL. On restart we re-derive
`preexisting=true` (dpkg-query still says installed) — but the original
preexisting was *also* true, so this one happens to be benign. The dangerous
direction is "we caused it to exist and crashed before journaling that fact."

**`systemd_unit.rs`** — first apply of `caddy.service`. Was `inactive +
disabled` before us. Provider captures backup with `prior_active=Some(false),
prior_enabled=Some(false)`, runs `systemctl enable && systemctl start`, KILL.
On restart: matches() returns true, skip. Journal has the empty backup. On
orphan unapply: `if !installed_by_us && !preexisting → return`, both are false,
returns. Service stays running forever, even when the bundle no longer claims it.

Cumulatively: the design's promise — "Golem only unapplies what Golem
installed" — is currently inverted under crash. Golem keeps things it installed
*and* loses the ability to restore things it overwrote.

### The structural fix

The Provider trait needs `apply` split into two phases, with the reconciler
journaling between them:

```rust
trait Provider {
    /// Probe reality. Set state.preexisting + state.backup. NO os mutation.
    /// Idempotent — safe to call repeatedly. Reconciler persists `state` after this returns.
    async fn capture(&self, spec: &ClaimSpec, state: &mut ClaimState) -> Result<()>;

    /// Mutate the OS. Set state.installed_by_us / state.content_hash. Reconciler persists after.
    /// May see a state where `installed_by_us=true` already (prior partial apply) — must be idempotent.
    async fn mutate(&self, spec: &ClaimSpec, state: &mut ClaimState) -> Result<()>;
}
```

Reconciler shape:

```rust
if state.last_applied.is_none() {            // first touch ever
    provider.capture(spec, &mut state).await?;
    self.store.put(&state)?;                 // preexisting + backup durable
}
provider.mutate(spec, &mut state).await?;
self.store.put(&state)?;
```

The retry loop should wrap `mutate`, not `capture`. `capture` is read-only and
shouldn't fail in practice; if it does, you skip the claim this tick rather than
fudging the journal.

An alternative that keeps the trait shape: pass the providers a
`journal: &mut dyn FnMut(&ClaimState)` callback so they can flush after the
read-but-before-mutate step. Less clean, same correctness.

### Secondary in the same area

- `state.rs:8-10` documents the correct discipline. The doc-comment and the
  caller disagree; right now the doc is a lie. Either update the comment to
  reflect the (broken) reality or, better, fix the caller.
- `reconcile.rs:142-145` reuses `recorded.get(&id)` if present. After a clean
  apply, the recorded state has accurate `preexisting`. After a crashed apply,
  the recorded state has the *stale tombstone*. The reconciler can't tell the
  difference. With the split-trait fix above, this stops mattering because
  `capture` runs idempotently any time `last_applied.is_none()`.
- The fast-path skip on `reconcile.rs:148-160` runs *before* the tombstone
  write, but that's fine — if reality already matches, no mutation happens, and
  on a future tick that doesn't match we'd journal-then-mutate. But note that a
  preexisting-and-already-matches claim never gets a journal entry. That means
  if the user later mutates the file out from under us and we re-apply, we
  treat it as a fresh first-touch with no backup of the *original* user content
  — only of the user-mutated content. Whether you care is a policy call.

---

## 2. Showstopper: Nickel → Rust serde tag mismatch

You asked specifically about this — the answer is no, the bundle does not round-trip.

Three independent layers of mismatch:

**Casing.** `golem-types/src/lib.rs:13` puts `rename_all = "snake_case"` on
`ProviderKind`, so `AptPackage` serializes as `"apt_package"`. `claims.ncl`
emits enum tags like `'AptPackage`, which Nickel exports as the literal string
`"AptPackage"`. Rust deserialize will see `"AptPackage"` and reject it as
unknown variant. Same for `'SystemdUnit` vs `"systemd_unit"`. `'File` vs
`"file"` is the only case-collapse-collision-free one and it still fails on
casing alone.

This affects both `Claim.id.kind` (a plain ProviderKind enum) and the
top-level `kind` discriminator emitted by ClaimSpec's adjacent tagging.

**Adjacently-tagged enum vs Nickel enum payload.** Rust's
`#[serde(tag = "kind", content = "spec")]` on `ClaimSpec` (`lib.rs:107-117`)
expects the wire shape:

```json
{ "kind": "file", "spec": { ...FileSpec... } }
```

Nickel `'File <FileSpec>` does not export to that shape. As of Nickel 1.7+,
enum constructors with payloads exist as a language feature, but JSON export of
`'Tag arg` does not produce a key called `kind` and a key called `spec`. You'd
have to write the projection by hand.

**Flatten interaction.** `Claim` has `#[serde(flatten)] spec: ClaimSpec`
(`lib.rs:127`). On the Rust side this hoists `kind` and `spec` to the top level
of the Claim, producing `{"id": ..., "kind": "...", "spec": {...}, "owners":
[...], "after": [...]}`. So now `kind` exists at two levels (inside `id` and at
the top of Claim) and the Nickel emitter has to produce both, with the same
casing convention, when it doesn't currently produce either.

### Fix options, in order of least-disruptive

**A. Pin Nickel to emit the wire shape directly.** Drop the enum-with-payload
contract; have `claims.ncl` define explicit string-tagged records. Constructor
helpers in `claims.ncl`:

```nickel
file_claim = fun id_key spec_record => {
  id = { kind = "file", key = id_key },
  kind = "file",
  spec = spec_record,
  owners = [],
  after = [],
},
```

This is ugly but it's the only path that keeps the Rust side as-is and
guarantees the wire shape. The contract becomes "by construction" rather than
"by type."

**B. Move Rust to PascalCase + a non-flatten Claim.** Drop `rename_all` and
`#[serde(flatten)]`, then tweak the wire shape so Nickel's natural enum export
maps to it. You'd need to know exactly what Nickel produces for `'Tag arg` —
probably `{"$tag": "Tag", "$value": ...}` — and pick a serde representation
that matches it. I don't think there's a clean adjacent/internal tag form that
matches Nickel's native shape, so this likely ends in a custom Deserialize on
ClaimSpec.

**C. Add a translation layer in `golemctl eval`.** After Nickel exports, run a
JSON transform inside golemctl that renames tags and flattens to the Rust shape.
This is the least principled but the easiest to land — it isolates the impedance
mismatch in one place and leaves both sides clean.

I'd take A. The Nickel contracts get less type-safe but the wire-format invariant
becomes visible and testable.

### Subordinate Nickel issues

- **`std.fail_with`** (`ingress.ncl:43`, `lib.ncl` patterns): not a stdlib
  function in any Nickel I can confirm. Modern Nickel uses contract-blame or
  `% error %` patterns; `std.fail_with` looks made up. Verify against your
  installed Nickel; if it's missing, replace with `std.contract.blame_with_message`
  or a contract that always fails.
- **`std.contract.apply contract "string-message" null`** (`lib.ncl:43-45`):
  `std.contract.apply`'s second argument is a Label, not a String. You'd need
  to construct a Label, or just use the type-annotation form `value | Contract`.
  This whole construct is more roundabout than necessary; `std.assert`
  (if present) or a contract-typed binding would read better.
- **`workload.ncl:40`** — `quadlet_body | String = render_quadlet ...`. Inside
  the contract, you're calling `render_quadlet` with `name image command user
  ports volumes env restart digest`. Those identifiers refer to the Workload's
  fields, but only in record-merge / recursive-record contexts is that legal.
  As written this looks like it's referring to free variables. Pretty sure you
  need `quadlet_body | String = render_quadlet name image command user ports
  volumes env restart digest` to be inside a `let rec` or to qualify them as
  `me.name`, `me.image`, etc. with a `me = self`-style binding. Or you have to
  rewrite render_quadlet to take the full record.
- **`workload.ncl:33`**: `'no'` (single-quoted with trailing apostrophe) is a
  Nickel enum tag with name `no'` — possibly intentional, but suspicious.
  systemd's `Restart=` value is `no` (no apostrophe). Either way `'no` would be
  cleaner; `std.string.from_enum 'no'` returns `"no'"` which is not what you want.
- The Quadlet expansion in **workload.ncl already produces File + SystemdUnit
  primitives directly** (lines 92-119). That's option (A) from
  `quadlet.rs:5-21`, and it means the agent-side `expand_quadlets` is dead code
  on the Nickel path. It only matters for hand-written bundles. Worth a comment
  or a feature flag to decide which path is canonical.

---

## 3. Build-blockers

The dependency tree compiles, with caveats. Walking your specific concerns:

**`ed25519-dalek` v2 API surface.** The code matches v2 correctly:
- `SigningKey::generate(&mut OsRng)` — needs the `rand_core` feature, which
  `golemctl/Cargo.toml:17` declares. ✓
- `SigningKey::from_bytes(&[u8;32])` — v2 takes `&SecretKey` (alias for
  `[u8;32]`), returns `Self` infallibly. `golemctl/src/main.rs:152` matches. ✓
- `Signature::from_bytes(&[u8;64])` — v2 returns `Self`, not Result.
  `bundle.rs:46` matches. ✓
- `VerifyingKey::from_bytes` returns Result. `bundle.rs:41` uses `?`. ✓
- `Signer` trait + `Verifier` trait imports check out. ✓

**`serde_json` canonicalization.** `bundle.rs:77-80` and `golemctl/src/main.rs:137-138`
both rely on `serde_json::to_value(&b)` producing a `Value::Object` with
BTreeMap-sorted keys. That's the default. The risk you flagged is real: if any
transitive dep enables the `preserve_order` feature (cargo features are
unioned), keys become insertion-ordered and signatures stop being canonical.

Run `cargo tree -e features --workspace -i serde_json` after a build to
confirm — none of `axum`, `reqwest`, `clap`, `tracing-subscriber`, `chrono`,
`rusqlite` enable `preserve_order` in their default builds, but a future minor
version of any of them could add it without you noticing. Cleanest fix: write
a deliberate canonical serializer (recursively walk `Value`, sort keys at every
level, emit compact JSON) instead of trusting feature semantics. ~30 lines.

**Crates declared but unused.** Cargo will compile fine but you're carrying weight:
- `zbus` — declared in `golemd/Cargo.toml:22`, never imported. Comments say
  "M1 shells out to systemctl, D-Bus is M2/M3" — so the dep is premature.
- `nix` — declared, unused (the `chown` TODO in `file.rs:129` would use it).
- `blake3` — workspace dep, unused in code; you use `sha2` everywhere.
- `thiserror` — workspace dep, unused. You use `anyhow` exclusively.
- `golem-types/Cargo.toml` declares `serde_json` and `blake3`; `lib.rs` uses
  neither. Drop them.

These won't block, but they balloon the binary and the build. If you actually
want a static musl drop-in <5MB, drop them now.

**`base64`, `fastrand`, `tempfile`, `hex`** — all declared, all used, all at
versions whose APIs you're calling correctly.

**Reconciler imports** in `reconcile.rs:22-30`: `ProviderKind` and `Scope` come
from `golem_types`, both `pub`. ✓

**`Claim.spec` flatten + adjacently tagged enum.** This compiles but the runtime
behavior with `#[serde(flatten)]` over an adjacently-tagged enum has caveats —
serde's flatten goes through a buffered deserialize and there have been bugs
historically. For an outbound serializer this is fine; for an inbound
deserializer with two `kind` keys (one in `id`, one at the top level of Claim,
both required to match), this works in current serde but the redundancy is
fragile. If you're going for option (A) Nickel fix, flatten the Claim by
flattening `id` too or get rid of the adjacent tagging.

---

## 4. Designed-in landmines

You asked for things you'll regret. In order of operational severity:

**Orphan sweep needs `last_spec`, you noted you don't store it before first
apply.** `reconcile.rs:103-110` skips orphan unapply if `last_spec` is None,
silently. That's tolerable for M1 (the only way last_spec is None is if the
journal was wiped between apply and unapply, which shouldn't happen) but it
means crashes during the *very first* apply of a claim that's then immediately
removed will leave a dangling resource forever. The fix is to set `last_spec`
in the tombstone *and* serialize whatever metadata the provider needs for
unapply at that moment. Right now `last_spec` is only written at the post-apply
put, which is the bug from issue 1 again wearing a different hat.

**Two operators racing on POST /bundle.** `http.rs:64-90` reads `prev_version`,
calls `load_signed`, then writes the new bundle. There's a TOCTOU between the
read of `prev_version` (`http.rs:68`) and the write (`http.rs:84`). Two
operators with v=2 and v=3 racing can result in v=2 winning if v=3 finishes its
verify first and v=2 finishes its lock acquisition first. The monotonic guard
becomes advisory, not enforceable. Hold the write lock for the whole verify
+ swap, or compare-and-swap the version inside the write critical section.
Also: same-version POSTs are silently accepted (`bundle.rs:58-64` uses `<`, not
`<=`), so two operators emitting "v=5" with different content will alternate
and you'll never notice from /status. Move to `<` to `<=` if you want
strictly-monotonic, or accept the ambiguity and document.

**`apt-get update` failing silently.** `apt.rs:120` does
`run("apt-get", &["update"]).await.ok();` — discards the result. On a node
with no internet, `update` fails, then `install` succeeds against the stale
cache (or fails for new packages). The install error bubbles up but you've
hidden the precondition. At minimum log the update failure at warn so the
operator can correlate. Consider: cache the last successful update timestamp
and refuse to re-run install if it's older than X without a successful update.

**`systemctl show -p A,B,C --value`** parsing in
`systemd_unit.rs:45-60`. With `--value`, systemctl prints values one per line in
the order requested — but the man page does not actually guarantee that order
matches the `-p` order. In practice it does, but if you want safety, drop
`--value` and parse `KEY=VALUE` with a small loop. Locale is *not* a concern
for these specific properties: `ActiveState`, `UnitFileState`, `LoadState` are
state-machine identifiers and are not localized. (For `Description` or
`Result`, locale would matter — but you don't read those.) Note also: the
`Some(_)` matching at line 73 means a unit with `LoadState=not-found` returns
None correctly, but a unit that's `loaded` but `inactive/disabled` returns
`Some((false, false))` and the reconciler will still consider it "Present."
That's the intent (the unit file exists, just isn't running), but verify it's
what you want — a not-yet-installed but loaded unit may not be what `Present`
should mean here.

**Quadlet expand drops user-supplied `after` for the unit half.**
`quadlet.rs:81-91`: the SystemdUnit's `after` becomes `vec![file_id.clone()]`,
discarding `claim.after`. So a Quadlet that says "after = [database
package]" loses the database dep on the unit. Merge: `let mut unit_after =
claim.after.clone(); unit_after.push(file_id.clone());`.

**`installed_by_us` semantics drift in systemd_unit.** `apply` sets
`installed_by_us=true` even when `preexisting=true` — i.e. when we toggled
state of a user-installed unit. Combined with the unapply check
`!installed_by_us && !preexisting`, the result is correct (we restore prior
state), but the field name lies. Either rename to `we_touched_it` or split:
`installed_by_us` (we caused it to exist) vs `modified_by_us` (we changed its
state). Documentation will thank you in a year.

**`render_quadlet` produces invalid quadlet on edge cases.** `workload.ncl:60-69`
joins ports / volumes / envs with `\n` then appends `\n` again — empty arrays
yield `\n\n` runs that podman tolerates but linters complain about. Minor.
More serious: `Restart=%{std.string.from_enum restart}` — if `restart = 'no'`
you get `Restart=no'` per the trailing-apostrophe issue above. systemd will
reject the unit.

**Bundle table in SQLite is unused.** `state.rs:38-43` declares a `bundle`
table; nothing reads or writes to it. Either wire it up (so a restart finds the
last-known bundle without needing the operator to re-push) or drop it. Right
now it's a Chesterton's fence.

**Reconciler tick has no timeout.** `apply_with_retries` in
`reconcile.rs:259-279` retries with exponential backoff up to 4 attempts. If
each provider call hangs (an apt-get blocked on dpkg lock; a systemctl on a
deadlocked unit) the tick can take effectively forever. Wrap each apply in
`tokio::time::timeout` (60s? 120s?) and surface the timeout as a TickReport
error; let the next tick try fresh.

**`apt.rs` lock is per-instance, not per-process.** `AptProvider::global()` is
called from `provider_for` (`providers/mod.rs:49`) every time a provider is
dispatched. The `OnceLock<Arc<Mutex>>` does dedupe — `get_or_init` returns the
same Arc — so the lock IS process-wide. ✓ But each call to `global()` allocates
a new `AptProvider` struct holding a clone of the Arc. Trivial cost; just noting
that the comment "Process-wide lock" is accurate even though the struct is
per-call.

**Signature verification message bytes** are computed twice — once on the CLI
side in `golemctl/main.rs:137-138`, once on the agent side in
`bundle.rs:77-80`. They have to byte-match. Both go through `to_value → to_vec`
which is the same canonicalization. ✓ But: any difference in workspace
serde_json features, anywhere, breaks one side and not the other. Centralize:
move `canonical_json(b: &Bundle) -> Vec<u8>` into `golem-types` so both
binaries use literally the same code. (The function exists in two places now,
which is the bug.)

**Daemon-reload heuristic** in `reconcile.rs:202-208` is keyed on file path
prefix. If anyone adds quadlets in `/etc/containers/systemd/users/` or under
`/usr/lib/systemd/system/` (drop-in dirs that systemd does scan), they're
silently outside the prefix list. Either generalize via a "this provider
declares it needs reload" hook on the Provider trait, or document the
restriction.

**`load_signed` runs `expand_quadlets` then `merge_claims`.** `bundle.rs:68-69`.
Expand happens first, so two workloads producing the same File + SystemdUnit
pair will dedupe correctly. ✓ But: expand creates handlers without checking
if a handler with the same source/targets already exists, so you'll
fire the same daemon-reload-and-restart twice in one tick if two workloads
target the same unit. Dedupe handlers in merge_claims.

**Post-`enable`/`start` failures are swallowed for `disable`/`stop`.**
`systemd_unit.rs:113-115` use `.ok()` on disable and stop. That's "best effort"
but masks real problems — e.g. a unit that won't stop because of a hung
ExecStop. At minimum log at warn.

---

## 5. M1 smoke test (no Nickel)

Hand-written. Files written to `smoke-test/` in your repo:

- `smoke-test/bundle-v1-install.json` — the install bundle.
- `smoke-test/bundle-v2-remove.json` — empty bundle that triggers orphan sweep.
- `smoke-test/run.sh` — the sequence: `keygen`, `sign`, run agent, verify, push v2, verify cleanup.

The bundle exercises:

1. `apt_package` claim for `caddy` with no `after` deps.
2. `file` claim for `/etc/caddy/Caddyfile` with `after: [caddy]`.
3. `systemd_unit` claim for `caddy.service` with `after: [Caddyfile]` and a
   handler that restarts `caddy.service` on Caddyfile change.

Run as:

```bash
cd smoke-test
./run.sh
```

Note: the `kill -9` mid-tick variant of this test is the one that will
demonstrate the journal-before-mutate bug from §1. After the bug is fixed the
intended assertion is "agent restart converges to the same end state as if no
crash happened, with no orphan resources and no preexisting-flag flips."

The signature comes from `golemctl sign`. If for some reason you want to skip
golemctl entirely and sign with openssl: ed25519 raw signing via openssl
requires extracting the 32-byte raw key from the PEM (the inner OCTET STRING in
the PrivateKeyInfo DER). Doable but annoying; not worth it when `golemctl
keygen` + `golemctl sign` exist.

---

## 6. Things that are actually fine

- The Provider trait surface (`observe` / `matches` / `apply` / `unapply` /
  `check`). Once the journal split lands, this is a clean abstraction. The
  separation of `observe` (cheap) from `matches` (potentially expensive) is good.
- The atomic file write in `file.rs:37-61`. tmpfile-in-same-dir, fsync,
  rename, parent fsync. Textbook.
- The reverse-topo orphan sweep ordering by ProviderKind in
  `reconcile.rs:86-93`. Good fallback when you can't reconstruct the original
  `after` graph for departed claims.
- The fast path on `reconcile.rs:148-153` (skip apply if matches). Necessary
  for steady-state cost.
- The handler debounce semantics. One daemon-reload per tick. Good.
- The static musl build via zig — clean and reproducible.
- The decision to use the bundle's signature as the trust boundary, not the
  socket. That's exactly right for a fleet pushed-to over Nebula.
- `chrono` with serde feature for timestamps in `ClaimState`. ✓
- ed25519-dalek v2 for keys + signatures. ✓
- `rusqlite` with `bundled` for static linking. ✓
- The "non-goals" list. Discipline.

---

## Punch list, ordered by what to do next

1. Split the Provider trait into `capture` + `mutate` and journal between. This
   is the design's central correctness commitment; you can't claim level-triggering
   over crash without it.
2. Pick a Nickel-Rust wire-format strategy (option A, B, or C from §2) and
   write a round-trip test: `golemctl eval examples/simple/config.ncl app-01 |
   target/.../golemd --bundle - --dry-run` that fails loudly on any
   serde error.
3. Centralize `canonical_json` in `golem-types` so both binaries can't diverge.
4. Drop dead deps (zbus, nix, blake3, thiserror, golem-types' serde_json &
   blake3). Re-add deliberately when needed.
5. Fix the http.rs version race (hold the write lock across verify+swap).
6. Wrap apply calls in `tokio::time::timeout`.
7. Merge `claim.after` into the Quadlet-expanded unit's after list.
8. Run the M1 smoke test under SIGKILL injection — it will fail until §1 is fixed.

The architecture is sound. The discipline is right. The implementation needs
the trait split and the wire-format glue before it'll do what the README says.
