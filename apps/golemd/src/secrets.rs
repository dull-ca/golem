use std::borrow::Cow;
use std::path::Path;

use aes_siv::aead::{Aead, KeyInit};
use aes_siv::{Aes256SivAead, Nonce};
use scroll_format::{Chunk, Secret, Text};

use crate::reconciler::{EnactError, EnactResult};

const KEY_BYTES: usize = 64;

pub struct Keyring {
    key: Option<FleetKey>,
}

struct FleetKey {
    key_id: String,
    cipher: Aes256SivAead,
}

#[derive(Debug)]
pub struct KeyFileError(String);

impl std::fmt::Display for KeyFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for KeyFileError {}

impl std::fmt::Debug for Keyring {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.key {
            Some(key) => write!(f, "Keyring(fleet key {})", key.key_id),
            None => f.write_str("Keyring(no fleet key)"),
        }
    }
}

impl Default for Keyring {
    fn default() -> Keyring {
        Keyring::without_key()
    }
}

impl Keyring {
    pub fn without_key() -> Keyring {
        Keyring { key: None }
    }

    pub fn from_key_file(path: &Path) -> Result<Keyring, KeyFileError> {
        let text = std::fs::read_to_string(path).map_err(|e| {
            KeyFileError(format!(
                "cannot read the fleet secret key {}: {e}",
                path.display()
            ))
        })?;
        let bytes = hex::decode(text.trim()).map_err(|_| malformed_key(path))?;
        if bytes.len() != KEY_BYTES {
            return Err(malformed_key(path));
        }
        let cipher = Aes256SivAead::new_from_slice(&bytes).map_err(|_| malformed_key(path))?;
        Ok(Keyring {
            key: Some(FleetKey {
                key_id: hex::encode(&blake3::hash(&bytes).as_bytes()[..8]),
                cipher,
            }),
        })
    }

    pub fn key_id(&self) -> Option<&str> {
        self.key.as_ref().map(|k| k.key_id.as_str())
    }

    pub fn open<'t>(&self, text: &'t Text, glyph_key: &str) -> EnactResult<Cow<'t, str>> {
        match text {
            Text::Plain(s) => Ok(Cow::Borrowed(s.as_str())),
            Text::Composed(chunks) => {
                let mut joined = String::new();
                for chunk in chunks {
                    match chunk {
                        Chunk::Lit(s) => joined.push_str(s),
                        Chunk::Hole(secret) => joined.push_str(&self.open_hole(secret, glyph_key)?),
                    }
                }
                Ok(Cow::Owned(joined))
            }
        }
    }

    /// Seal host state golem is about to journal, so the durable record is
    /// ciphertext at rest even though the file it was read from is not. Applied
    /// to *every* prior file capture on a keyed host rather than only to files
    /// whose desired glyph carries a secret: golemd cannot tell from the prior
    /// bytes whether an earlier revision put a credential there, and a rule that
    /// guessed would leak exactly on the revision that stops using a secret.
    /// A keyless host has no key to seal with — and equally no way to have
    /// enacted a secret — so it journals what it always did.
    pub fn seal(&self, plaintext: &str, glyph_key: &str) -> EnactResult<Text> {
        let Some(key) = &self.key else {
            return Ok(Text::Plain(plaintext.to_string()));
        };
        let ciphertext = key
            .cipher
            .encrypt(&Nonce::default(), plaintext.as_bytes())
            .map_err(|_| {
                EnactError::Fatal(format!(
                    "{glyph_key}: fleet key {} could not seal the prior host state for the \
                     journal, and golem will not record it in the clear",
                    key.key_id
                ))
            })?;
        Ok(Text::composed(vec![Chunk::Hole(Secret::Sealed {
            key_id: key.key_id.clone(),
            ciphertext,
        })]))
    }

    fn open_hole(&self, secret: &Secret, glyph_key: &str) -> EnactResult<String> {
        let (key_id, ciphertext) = match secret {
            Secret::Reference { provider, key } => {
                return Err(EnactError::Fatal(format!(
                    "{glyph_key}: `{key}` is a host-side reference to provider `{provider}`, \
                     and host-side secret resolution is not implemented in this golemd — \
                     recompile the manifest so the value ships sealed"
                )))
            }
            Secret::Sealed { key_id, ciphertext } => (key_id, ciphertext),
        };
        let Some(key) = &self.key else {
            return Err(EnactError::Fatal(format!(
                "{glyph_key}: this value is sealed under fleet key {key_id}, but no fleet \
                 secret key is configured on this host — provision one and name it with \
                 `--secrets-key-file <FILE>` or `[secrets] key_file` in golemd.toml"
            )));
        };
        if key.key_id != *key_id {
            return Err(EnactError::Fatal(format!(
                "{glyph_key}: this value is sealed under fleet key {key_id}, but this host \
                 holds fleet key {} — the manifest was compiled for a different fleet, or \
                 against a key this host has not been rotated onto",
                key.key_id
            )));
        }
        let plaintext = key
            .cipher
            .decrypt(&Nonce::default(), ciphertext.as_slice())
            .map_err(|_| {
                EnactError::Fatal(format!(
                    "{glyph_key}: the value sealed under fleet key {key_id} did not decrypt — \
                     the manifest is corrupt or was sealed under a different key with the \
                     same id"
                ))
            })?;
        String::from_utf8(plaintext).map_err(|_| {
            EnactError::Fatal(format!(
                "{glyph_key}: the value sealed under fleet key {key_id} decrypted to bytes \
                 that are not valid UTF-8"
            ))
        })
    }
}

fn malformed_key(path: &Path) -> KeyFileError {
    KeyFileError(format!(
        "the fleet secret key {} must be {} hexadecimal characters (a {KEY_BYTES}-byte AES-SIV key)",
        path.display(),
        KEY_BYTES * 2
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "00112233445566778899aabbccddeeff\
                       00112233445566778899aabbccddeeff\
                       ffeeddccbbaa99887766554433221100\
                       ffeeddccbbaa99887766554433221100";

    fn key_file(dir: &tempfile::TempDir, hex: &str) -> std::path::PathBuf {
        let path = dir.path().join("fleet.key");
        std::fs::write(&path, hex).unwrap();
        path
    }

    fn sealed(plaintext: &str, key_hex: &str) -> Secret {
        let bytes = hex::decode(key_hex).unwrap();
        let cipher = Aes256SivAead::new_from_slice(&bytes).unwrap();
        Secret::Sealed {
            key_id: hex::encode(&blake3::hash(&bytes).as_bytes()[..8]),
            ciphertext: cipher
                .encrypt(&Nonce::default(), plaintext.as_bytes())
                .unwrap(),
        }
    }

    fn message(result: EnactResult<Cow<'_, str>>) -> String {
        match result {
            Err(EnactError::Fatal(m)) => m,
            other => panic!("expected a fatal refusal, got {other:?}"),
        }
    }

    #[test]
    fn plain_text_needs_no_key_and_is_not_copied() {
        let text = Text::Plain("just text".into());
        let opened = Keyring::without_key().open(&text, "file:/etc/x").unwrap();
        assert!(matches!(opened, Cow::Borrowed("just text")));
    }

    #[test]
    fn a_composed_value_joins_literals_and_opened_holes_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let keyring = Keyring::from_key_file(&key_file(&dir, KEY)).unwrap();
        let text = Text::composed(vec![
            Chunk::Lit("password=".into()),
            Chunk::Hole(sealed("hunter2", KEY)),
            Chunk::Lit("\n".into()),
        ]);
        assert_eq!(
            keyring.open(&text, "file:/etc/x").unwrap(),
            "password=hunter2\n"
        );
    }

    #[test]
    fn sealing_host_state_round_trips_and_never_holds_the_plaintext() {
        let dir = tempfile::tempdir().unwrap();
        let keyring = Keyring::from_key_file(&key_file(&dir, KEY)).unwrap();
        let sealed = keyring.seal("password=hunter2\n", "file:/etc/x").unwrap();
        assert!(matches!(sealed, Text::Composed(_)));
        assert!(!format!("{sealed:?}").contains("hunter2"));
        assert_eq!(
            keyring.open(&sealed, "file:/etc/x").unwrap(),
            "password=hunter2\n"
        );
    }

    #[test]
    fn sealing_the_same_host_state_twice_gives_the_same_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let keyring = Keyring::from_key_file(&key_file(&dir, KEY)).unwrap();
        assert_eq!(
            keyring.seal("same", "file:/etc/x").unwrap(),
            keyring.seal("same", "file:/etc/y").unwrap()
        );
    }

    #[test]
    fn a_keyless_keyring_journals_host_state_verbatim() {
        assert_eq!(
            Keyring::without_key()
                .seal("was here", "file:/etc/x")
                .unwrap(),
            Text::Plain("was here".into())
        );
    }

    #[test]
    fn the_key_id_is_the_first_eight_bytes_of_the_keys_blake3() {
        let dir = tempfile::tempdir().unwrap();
        let keyring = Keyring::from_key_file(&key_file(&dir, KEY)).unwrap();
        let expected = hex::encode(&blake3::hash(&hex::decode(KEY).unwrap()).as_bytes()[..8]);
        assert_eq!(keyring.key_id(), Some(expected.as_str()));
    }

    #[test]
    fn a_key_file_with_trailing_whitespace_still_loads() {
        let dir = tempfile::tempdir().unwrap();
        let with_newline = key_file(&dir, &format!("{KEY}\n"));
        assert!(Keyring::from_key_file(&with_newline).is_ok());
    }

    #[test]
    fn a_short_key_file_is_refused_by_path_and_length() {
        let dir = tempfile::tempdir().unwrap();
        let path = key_file(&dir, "0011223344");
        let err = Keyring::from_key_file(&path).unwrap_err().to_string();
        assert!(err.contains(&path.display().to_string()));
        assert!(err.contains("128 hexadecimal characters"));
    }

    #[test]
    fn a_missing_key_file_is_refused_by_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("absent.key");
        let err = Keyring::from_key_file(&path).unwrap_err().to_string();
        assert!(err.contains(&path.display().to_string()));
    }

    #[test]
    fn a_sealed_hole_with_no_key_names_both_ways_to_configure_one() {
        let text = Text::composed(vec![Chunk::Hole(sealed("hunter2", KEY))]);
        let msg = message(Keyring::without_key().open(&text, "file:/etc/x"));
        assert!(msg.contains("--secrets-key-file"));
        assert!(msg.contains("[secrets] key_file"));
        assert!(msg.contains("file:/etc/x"));
        assert!(!msg.contains("hunter2"));
    }

    #[test]
    fn a_key_id_mismatch_names_the_manifests_key_and_the_hosts() {
        let dir = tempfile::tempdir().unwrap();
        let other = "ffeeddccbbaa99887766554433221100\
                     ffeeddccbbaa99887766554433221100\
                     00112233445566778899aabbccddeeff\
                     00112233445566778899aabbccddeeff";
        let keyring = Keyring::from_key_file(&key_file(&dir, other)).unwrap();
        let text = Text::composed(vec![Chunk::Hole(sealed("hunter2", KEY))]);
        let msg = message(keyring.open(&text, "file:/etc/x"));
        let manifest_key_id =
            hex::encode(&blake3::hash(&hex::decode(KEY).unwrap()).as_bytes()[..8]);
        assert!(msg.contains(&manifest_key_id));
        assert!(msg.contains(keyring.key_id().unwrap()));
        assert!(!msg.contains("hunter2"));
    }

    #[test]
    fn a_corrupt_ciphertext_is_refused_rather_than_written() {
        let dir = tempfile::tempdir().unwrap();
        let keyring = Keyring::from_key_file(&key_file(&dir, KEY)).unwrap();
        let text = Text::composed(vec![Chunk::Hole(Secret::Sealed {
            key_id: keyring.key_id().unwrap().to_string(),
            ciphertext: vec![0; 32],
        })]);
        assert!(message(keyring.open(&text, "file:/etc/x")).contains("did not decrypt"));
    }

    #[test]
    fn a_reference_names_its_provider_and_key_and_says_host_side_is_unbuilt() {
        let dir = tempfile::tempdir().unwrap();
        let keyring = Keyring::from_key_file(&key_file(&dir, KEY)).unwrap();
        let text = Text::composed(vec![Chunk::Hole(Secret::Reference {
            provider: "onepassword".into(),
            key: "DB_PASSWORD".into(),
        })]);
        let msg = message(keyring.open(&text, "file:/etc/x"));
        assert!(msg.contains("onepassword"));
        assert!(msg.contains("DB_PASSWORD"));
        assert!(msg.contains("host-side"));
    }

    #[test]
    fn a_reference_is_refused_even_where_a_key_is_configured() {
        let text = Text::composed(vec![Chunk::Hole(Secret::Reference {
            provider: "keyring".into(),
            key: "K".into(),
        })]);
        assert!(message(Keyring::without_key().open(&text, "file:/etc/x")).contains("keyring"));
    }
}
