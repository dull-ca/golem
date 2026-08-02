# 0047 — Typed secrets on the wire: sealed now, referenced later

## Status

Proposed 2026-08-02. Extends the manifest model (ADR 0012/0013) and bumps
`format_version` 4 → 5. Depends on ADR 0042's key-distribution channel.
Constrained by ADR 0004 (the IR is inert, fully-evaluated data) and by
content addressing (ADR 0012/0015), which this must not weaken.

## Context

Every deployment golem describes needs values that must not sit in source
control: a database password, an admin credential, an API token. Today the
LimeSurvey example carries literals in its `.emet`, which is honest about
what it is and unusable for anything real.

secretspec (already in the toolchain) declares required secrets in a
`secretspec.toml` and resolves them from a provider — keyring, env, dotenv,
1Password — with a Rust library `emetc` can link. That answers *where a
value comes from*. It leaves two questions that are golem's own.

**What reaches the wire.** A manifest is compiled once and applied to many
hosts; it is an artifact that may be stored, cached, or handed to CI. If
`emetc` resolves a secret to a plain `String`, the manifest becomes
secret-bearing and cannot be treated as an ordinary build output.

**What content addressing must keep meaning.** golemd diffs by content id.
Whatever a secret becomes on the wire, two properties have to hold: the same
secret must produce the same bytes (or every build re-enacts every dependent
unit), and a rotated secret must produce different bytes (or a rotation
silently never reaches the fleet).

A third pressure is visible but not yet urgent: some providers are better
placed on the host — an instance role, a TPM, a socket to a local agent — in
which case the value should never ship in any form. That is a different
mechanism, not a different encoding, and designing as though it does not
exist would force a second format break to add it.

## Decision

The wire gains a **typed secret**, and the field types that can hold one
become a sum rather than a bare `String`.

```text
Secret
  = Sealed    { key_id : String, ciphertext : Bytes }
  | Reference { provider : String, key : String }     // reserved; not enacted yet
```

**`Sealed` is what ships in this ADR.** `emetc` resolves the value through
secretspec at compile time and encrypts it to the fleet's key. Encryption is
**deterministic** (a misuse-resistant AEAD, or a nonce derived from the key
and plaintext) — not a performance choice but the property that keeps
content addressing honest: identical secret, identical bytes, identical
content id, no diff; rotated secret, new bytes, new content id, and golem
re-enacts exactly the units that depend on it.

**`Reference` is defined now and rejected at enact time.** It names a
provider and a key for a host-side resolver to answer. Reserving the variant
costs one unused arm today and saves a `format_version` break when host-side
providers arrive.

**The authoring surface is `Secretspec.get "key"`** — named for the provider
system, not the concept, so a later `Vault.get` sits beside it rather than
fighting for the name. A key must be declared in `secretspec.toml`;
`emetc` rejects an undeclared key by name, listing the declared ones, before
any provider is consulted.

**Key distribution reuses ADR 0042's channel.** The fleet key is deployed
exactly as the bearer token is — root-owned, mode 0600, provisioned by the
harness — because that mechanism exists, is understood, and already defines
the trust boundary this inherits.

## Consequences

- A manifest stops being secret-bearing: it can be stored, cached, diffed
  and shipped to CI. That is the point of the change.
- **The host still holds plaintext.** golemd decrypts to write
  `Environment=…` or a config file, and its journal records prior contents
  inline so removal stays exact (ADR 0015). This protects the artifact, not
  the box. Anyone reading "my secrets are encrypted" as "safe on the host"
  is wrong, and the docs must say so plainly.
- `emetc` becomes non-hermetic: the same source yields a different manifest
  when a secret changes, and compiling requires provider access plus the
  fleet key. The ADR 0043 docs-examples harness must therefore never use
  `Secretspec.get`, and CI needs the key to build any manifest that does.
- A `Reference` in a manifest is a hard error at enact, naming the provider
  and saying host-side resolution is unbuilt — an honest refusal rather than
  a silent skip.
- When `Reference` is enacted one day it will forfeit rotation-triggers-
  reconcile: its content id covers the reference, not the value, so a
  rotated secret produces no diff. That is inherent to resolving on the
  host, and the reason `Sealed` is the default rather than a stepping stone.
- `format_version` 4 → 5. The encoding is non-self-describing postcard, so
  this is a real break: a v5 manifest is undecodable by a v4 golemd, which
  is what the version guard exists for.
