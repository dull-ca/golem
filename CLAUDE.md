# Golem — project conventions

## The wire format is an implementation detail

**The bytes on the wire are not the product.** What matters is the
*conceptual model* the wire format encodes — Bundle, Claim, the claim
kinds (File, AptPackage, SystemdUnit, Quadlet, NftFragment, DnsRecord),
their specs, ownership, ordering, handlers. That model is the contract
between layer 2 (`golemctl`) and layer 3 (`golemd`). The bytes are
whatever's expedient.

Today the wire format is canonical JSON because Nickel's natural export
is JSON and we wanted to ship M1 fast. **This will change.** Plan of
record is to move to a binary, statically-typed format with strong
generated types on both sides (protobuf, msgpack-with-schema, CBOR-CDDL,
Cap'n Proto, FlatBuffers — exact choice TBD; the criteria are: good
typing story, easy parsing in Rust, generated types that are hard to
misuse). When that happens, the *model* doesn't change; the serializer
does.

### What this means for code and docs

- Treat the JSON shape in `crates/golem-types/`, `nickel/claims.ncl`,
  and the bundle-format reference as *one representation* of the model,
  not as the model itself. The Nickel contracts and the Rust types are
  the source of truth; the JSON is what they currently serialize to.
- Don't write documentation that elevates JSON details (key ordering,
  canonical form, base64 conventions) as the headline. Lead with the
  concepts; mention the current encoding as an implementation note.
- Don't add features that only make sense for JSON (e.g. a "raw extra
  field" escape hatch that hand-written JSON tools rely on). Anything
  added has to survive the binary-format migration.
- Canonical-form work in `crates/golem-types/src/canonical.rs` is
  load-bearing *today* for signature stability, but the long-term
  story is "the serializer produces canonical bytes by construction" —
  not "we re-canonicalize after serializing." A binary schema that
  sorts fields deterministically gets us there cheaply.
- When writing new docs about the wire format, frame it as: "here's
  the conceptual model; here's how the current encoding represents it;
  the encoding is expected to change to a typed binary format."

### The model worth defending

These are the concepts that must survive the format migration:

- **Bundle**: a per-host, monotonically-versioned, signed envelope.
- **Claim**: an idempotent, declarative assertion of one resource's
  desired state, identified by `(kind, key)`, with `owners` and `after`.
- **Capture**: durable, one-shot-per-claim, the source of truth for
  honest unapply.
- **Handler**: source-claim-changed → restart-target-units.
- **Claim kinds**: File, AptPackage, SystemdUnit, Quadlet, NftFragment,
  DnsRecord. Adding a kind is a versioned schema change; the agent
  rejects bundles whose schema version it doesn't understand.

Anything in the JSON shape that doesn't correspond to one of those —
key ordering, whitespace, base64 framing — is not the model. It can be
freely traded against ergonomics in the binary format.

## Skill routing

When the user's request matches an available skill, invoke it via the
Skill tool. Don't proactively run gstack ceremony (telemetry, gbrain,
AskUserQuestion decision briefs, codex review gates) unless explicitly
asked. The user's auto mode is on; prefer action over planning.

## Git

Never use git unless explicitly asked. No commits, no pushes, no
branches, no resets — even when a refactor obviously wants a commit.
The user decides when to commit.
