//! The fleet secret key: how a key file's text becomes a cipher, how a key
//! names itself in a [`Secret::Sealed`]'s `key_id`, and how a value is sealed
//! and unsealed under it (ADR 0047).
//!
//! One definition rather than two. `emetc` seals and `golemd` unseals, and a
//! disagreement between them about any of key length, hex encoding, `key_id`
//! derivation, cipher, or nonce produces manifests the daemon cannot open —
//! at enact time, on a host, with the compile long finished. Both ends reach
//! the seal only through this type, so there is no second implementation to
//! drift from.
//!
//! No filesystem here, as everywhere else in this crate. Each end reads its own
//! key file — over different flags, into different error types — and passes the
//! text to [`FleetKey::from_hex`].

use std::fmt;

use aes_siv::aead::{Aead, KeyInit};
use aes_siv::{Aes256SivAead, Nonce};

use crate::scroll::Secret;

/// The key length AES-256-SIV takes: two 256-bit halves, one deriving the
/// synthetic IV and one keying the counter mode.
pub const FLEET_KEY_BYTES: usize = 64;

/// A loaded fleet key — the cipher plus the `key_id` that a
/// [`Secret::Sealed`] carries to say which key it was sealed under.
pub struct FleetKey {
    key_id: String,
    cipher: Aes256SivAead,
}

/// The key text was not [`FLEET_KEY_BYTES`] of hex. [`Display`](fmt::Display)
/// renders the requirement as a clause, so a caller that knows where the text
/// came from can name it: `format!("the fleet secret key {path} {why}")`.
#[derive(Debug, PartialEq, Eq)]
pub struct MalformedFleetKey;

#[derive(Debug, PartialEq, Eq)]
pub struct SealFailed;

/// Why a [`Secret::Sealed`] did not yield its plaintext. Each end supplies its
/// own prose: `golemd` names the glyph being enacted and both key ids, which
/// this type has no access to.
#[derive(Debug, PartialEq, Eq)]
pub enum UnsealError {
    /// The value was sealed under a different key than the one held.
    KeyIdMismatch,
    /// The ciphertext failed authentication — corrupt, or sealed under a
    /// different key that happens to share this one's `key_id`.
    Undecryptable,
    NotUtf8,
}

impl fmt::Display for MalformedFleetKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "must be {} hexadecimal characters (a {FLEET_KEY_BYTES}-byte AES-SIV key)",
            FLEET_KEY_BYTES * 2
        )
    }
}

impl std::error::Error for MalformedFleetKey {}

impl fmt::Debug for FleetKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FleetKey({})", self.key_id)
    }
}

impl FleetKey {
    /// Read a key from the text of a key file: [`FLEET_KEY_BYTES`] as
    /// lowercase hex, surrounding whitespace trimmed so a file's trailing
    /// newline is not mistaken for part of the key.
    pub fn from_hex(text: &str) -> Result<FleetKey, MalformedFleetKey> {
        let bytes = hex::decode(text.trim()).map_err(|_| MalformedFleetKey)?;
        if bytes.len() != FLEET_KEY_BYTES {
            return Err(MalformedFleetKey);
        }
        let cipher = Aes256SivAead::new_from_slice(&bytes).map_err(|_| MalformedFleetKey)?;
        Ok(FleetKey {
            // NOTE: eight bytes of BLAKE3 over the key, hex-encoded. This
            // selects which key to try and is compared before decrypting; it is
            // not the integrity check — AES-SIV's authentication is, which is
            // why truncating to 64 bits costs nothing. A collision makes an
            // undecryptable value report a corrupt ciphertext rather than the
            // wrong fleet.
            key_id: hex::encode(&blake3::hash(&bytes).as_bytes()[..8]),
            cipher,
        })
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    /// Encrypt to a [`Secret::Sealed`] naming this key.
    ///
    /// **Deterministic**: the same key and plaintext always give the same
    /// ciphertext, so a secret-bearing glyph keeps its content id across builds
    /// and moves exactly when the secret is rotated (ADR 0047). That is what
    /// the fixed nonce buys, and AES-SIV is the misuse-resistant AEAD chosen so
    /// that reusing one is defined rather than catastrophic. The cost is
    /// inherent to the choice: equal ciphertexts reveal equal plaintexts.
    pub fn seal(&self, plaintext: &[u8]) -> Result<Secret, SealFailed> {
        let ciphertext = self
            .cipher
            .encrypt(&Nonce::default(), plaintext)
            .map_err(|_| SealFailed)?;
        Ok(Secret::Sealed {
            key_id: self.key_id.clone(),
            ciphertext,
        })
    }

    /// Decrypt a [`Secret::Sealed`]'s parts back to the text `seal` was given.
    /// Sealed values are always UTF-8 in golem — they become file contents,
    /// unit lines, environment values — so bytes that are not are a refusal
    /// rather than a lossy conversion.
    pub fn unseal(&self, key_id: &str, ciphertext: &[u8]) -> Result<String, UnsealError> {
        if self.key_id != key_id {
            return Err(UnsealError::KeyIdMismatch);
        }
        let plaintext = self
            .cipher
            .decrypt(&Nonce::default(), ciphertext)
            .map_err(|_| UnsealError::Undecryptable)?;
        String::from_utf8(plaintext).map_err(|_| UnsealError::NotUtf8)
    }
}
