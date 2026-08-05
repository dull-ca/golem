# Typed Secrets Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** A secret declared in `secretspec.toml` can be written as `Secretspec.get "key"` in Emet, ships in the manifest as a deterministically-encrypted `Sealed` value, and is decrypted by golemd at enact — so a manifest is no longer secret-bearing while content addressing keeps telling the truth.

**Architecture:** ADR 0047. `scroll-format` gains a `Secret` sum (`Sealed` now, `Reference` reserved) and the value-bearing glyph fields become a sum rather than `String`. `emetc` resolves through secretspec's Rust library at compile time and seals with a deterministic AEAD. golemd unseals at enact and refuses `Reference`. The fleet key rides ADR 0042's existing provisioning channel.

**Tech Stack:** Rust (`scroll-format`, `emet`, `golemd`), the `secretspec` crate, a misuse-resistant AEAD (AES-SIV or equivalent), Python for the harness.

## Global Constraints

- Governing decision: `docs/adr/0047-typed-secrets-on-the-wire.md`. Where it is silent, prefer the option that keeps content addressing honest.
- **Determinism is the load-bearing property.** Same secret ⇒ identical ciphertext ⇒ identical content id. A random nonce silently re-enacts every dependent unit on every build; a test must pin this.
- `lw:implementer` writes ZERO comments; `lw:documenter` follows.
- Every existing `.emet` that uses no secret must compile to a BYTE-IDENTICAL manifest — prove against `git archive HEAD`. The format bump changes the header, so compare *decoded* scrolls where the raw bytes must differ, and say which you did.
- `cargo test --workspace` green including `apps/emet/tests/docs_examples.rs` (ADR 0043). That harness must never use a secret — it has no key and no provider.
- The plaintext still reaches the host and its journal. No doc may imply otherwise.

---

### Task 1: The wire type

**Files:** `libs/scroll-format/src/scroll.rs`, `manifest.rs`; tests in that crate.

- `Secret = Sealed { key_id: String, ciphertext: Vec<u8> } | Reference { provider: String, key: String }`.
- The value-bearing fields become a sum (`Plain(String) | Secret(Secret)`): an `EnvVar` value, a file's `contents`, and a `lineInFile` line. Paths, names, unit names and modes stay `String` — a secret is a *value*, never an identifier.
- `format_version` 4 → 5, with the history line extended.
- Content id must cover the sealed bytes; a `Plain` value's encoding must be unchanged from v4 so an unrelated glyph's content id does not move.

- [ ] Failing tests: round-trip both variants; a `Plain` glyph's content id equals its v4 value; two `Sealed` values with identical ciphertext have equal content ids and differing ciphertext differ.
- [ ] Implement; run; commit.

### Task 2: `Secretspec.get` in Emet

**Files:** `apps/emet/src/{prelude,infer,eval}.rs`, `main.rs`; new tests.

- `Secretspec.get "key"` as a module-qualified builtin (ADR 0006), resolved at **compile time** through the secretspec Rust library.
- Read `secretspec.toml` (auto-detected by walking up, as the CLI does). An undeclared key is a compile error naming it and listing the declared keys. A declared key the provider cannot supply is a distinct error naming the provider.
- Seal deterministically with the fleet key. The key is a compile input — decide flag vs env (`--secret-key` / `GOLEM_SECRET_KEY_FILE`) and justify it. A program using no secret must need neither key nor provider.

**Interpolation is the crux (ADR 0047, `Text = Plain | Composed`).** A secret must survive `"Environment=PW=${Secretspec.get "db"}"` by becoming `Composed [Lit "Environment=PW=", Hole (Sealed …)]` — literal chunks with sealed holes, so the surrounding text stays readable in the manifest and in `golemctl plan`. Sealing the whole string is explicitly rejected by the ADR.

That means a secret-derived value must be distinguishable from an ordinary string all the way from `Secretspec.get` to lowering, through `++`, `String.concat`, and `${}` interpolation. Decide how — a tainted runtime string that lowers to `Composed`, or a distinct type with its own operations — and justify the choice in your report. The surface must stay usable: `lib/Quadlet.emet` builds env lines by interpolation today and should keep working unchanged.

**A secret must never reach an identifier position.** A path, a unit name, a scroll name, or a mode is an identifier, not a value. If a secret-derived value reaches one, that is an error naming the field — never a silent unsealing, never a stringified ciphertext. If your chosen design makes this a type error rather than a lowering error, say so; that is strictly better.

- [ ] Failing tests: undeclared key (message text); missing provider (message text); determinism — compile twice, identical manifest bytes; interpolation produces `Composed` with the literal chunks intact and only the secret sealed; a secret in a path is refused; a program with no secret compiles with no key and no provider configured.
- [ ] Implement; run; commit.

### Task 3: golemd unseals

**Files:** `apps/golemd/src/{config,reconcilers,foreman}.rs`.

- `[secrets] key_file` in `golemd.toml` plus a flag, mirroring `[auth] token_file` (ADR 0042).
- Unseal at enact, as late as possible: the plaintext should exist only where the reconciler writes it.
- `Reference` is a hard error naming provider and key, saying host-side resolution is unbuilt.
- A `Sealed` value with no key configured, or a `key_id` that does not match, is a typed actionable error — never a panic, never a silent skip.
- **Redaction:** progress events, the plan render, and error messages must never carry plaintext. Check what a failing file-write reports today.

- [ ] Failing tests: unseal round-trip through a real reconcile; wrong/absent key; `Reference` refused; a report or event containing a secret's plaintext fails the test.
- [ ] Implement; run; commit.

### Task 4: Key provisioning in the harness

**Files:** `apps/fleet/{token,deploy,config}.py` and tests.

- Generate `.fleet/golem-secret-key` once (0600), deploy to `/etc/golem/secret-key` root:root 0600, and point `golemd.toml` at it — the same shape `ensure_token`/`_ssh_write_secret` already use, including never letting the value reach an error message.
- `emetc` must find the same key when the harness drives a build.

- [x] Tests mirroring the token ones; commit.

Done. `ensure_secret_key` sits beside `ensure_token` in `apps/fleet/token.py`
(64 random bytes as 128 hex characters, `O_EXCL` at 0600, validated on read
back); `Paths.secret_key_file` is `.fleet/golem-secret-key`; `deploy_golemd`
writes `/etc/golem/secret-key` root:root 0600 through the same
`_ssh_write_secret` the token uses, and `golemd_config_toml` names it under
`[secrets]`. `compile_manifest` exports `GOLEM_SECRET_KEY_FILE` so the harness
seals with the key its guests unseal with — the path travels, the bytes never
do.

### Task 5: Use it, prove it, document it

- [x] Convert `examples/limesurvey/` from literal passwords to `Secretspec.get`, with a `secretspec.toml` declaring the keys.
- [ ] Live on the VM fleet: apply to `manta`, confirm the container starts and the value inside it is correct, then confirm the *manifest* contains no plaintext (grep the bytes).
- [x] `lw:documenter`: ADR 0047 Proposed → Accepted; `apps/emet/CLAUDE.md`; the language reference; a site page on the trust boundary that says plainly what this does and does not protect.

The conversion also restructured the example onto nested config types
(`LimeConfig` holding `LimeSurvey` and `LimeSurveyDatabase`), each pairing a
defaults record with a required-fields argument so a caller overrides a default
by record update (ADR 0044) and a password has no default to omit. The trust
boundary went into the existing `explanation/trust.mdx` rather than a new page,
and `Secretspec.get` into `reference/language/prelude.mdx`.

**The manifest-has-no-plaintext half is proven, the live half is not.** Compiling
`examples/limesurvey/main.emet` against a dotenv provider and grepping the
manifest bytes finds neither canary value, and two builds are byte-identical
(BLAKE3 `7ce34c05…`), pinning determinism. Applying to `manta` and reading the
value back out of the running container is still owed.
