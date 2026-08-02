use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt;
use std::ops::Range;
use std::path::{Path, PathBuf};

use aes_siv::aead::{Aead, KeyInit};
use aes_siv::{Aes256SivAead, Nonce};

use crate::ir::{Chunk, Secret, Text};

pub const GET: &str = "Secretspec.get";
pub const IS_SECRET: &str = "String.isSecret";

const MANIFEST_FILE: &str = "secretspec.toml";
const KEY_BYTES: usize = 64;

#[derive(Clone, Debug, Default)]
pub struct SecretOptions {
    pub key_file: Option<PathBuf>,
    pub provider: Option<String>,
    pub profile: Option<String>,
}

#[derive(Clone)]
pub enum TextPiece {
    Literal(String),
    Resolved(ResolvedSecret),
    Sealed(Secret),
}

#[derive(Clone)]
pub struct ResolvedSecret {
    key: String,
    plaintext: String,
}

impl ResolvedSecret {
    pub fn key(&self) -> &str {
        &self.key
    }
}

impl fmt::Debug for ResolvedSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<secret {}>", self.key)
    }
}

impl fmt::Debug for TextPiece {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TextPiece::Literal(s) => f.debug_tuple("Literal").field(s).finish(),
            TextPiece::Resolved(secret) => write!(f, "{secret:?}"),
            TextPiece::Sealed(_) => f.write_str("<sealed secret>"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SecretText(Vec<TextPiece>);

impl SecretText {
    pub fn of_secret(secret: ResolvedSecret) -> SecretText {
        SecretText(vec![TextPiece::Resolved(secret)])
    }

    pub fn derived_from(plaintext: String, sources: &[&SecretText]) -> SecretText {
        let key = sources
            .iter()
            .flat_map(|text| text.secret_keys())
            .collect::<Vec<_>>()
            .join("+");
        SecretText::of_secret(ResolvedSecret { key, plaintext })
    }

    pub fn secret_keys(&self) -> Vec<String> {
        self.0
            .iter()
            .filter_map(|piece| match piece {
                TextPiece::Resolved(secret) => Some(secret.key.clone()),
                TextPiece::Literal(_) | TextPiece::Sealed(_) => None,
            })
            .collect()
    }

    pub fn pieces(&self) -> &[TextPiece] {
        &self.0
    }

    pub fn plaintext(&self) -> Option<String> {
        let mut out = String::new();
        for piece in &self.0 {
            match piece {
                TextPiece::Literal(s) => out.push_str(s),
                TextPiece::Resolved(secret) => out.push_str(&secret.plaintext),
                TextPiece::Sealed(_) => return None,
            }
        }
        Some(out)
    }

    pub fn seal(&self) -> Result<Text, String> {
        let mut chunks = Vec::with_capacity(self.0.len());
        for piece in &self.0 {
            chunks.push(match piece {
                TextPiece::Literal(s) => Chunk::Lit(s.clone()),
                TextPiece::Resolved(secret) => Chunk::Hole(seal(&secret.plaintext)?),
                TextPiece::Sealed(secret) => Chunk::Hole(secret.clone()),
            });
        }
        Ok(Text::composed(chunks))
    }
}

pub enum Composed {
    Plain(String),
    Tainted(SecretText),
}

pub fn compose(pieces: Vec<TextPiece>) -> Composed {
    if pieces.iter().any(carries_secret) {
        Composed::Tainted(SecretText(merge_literals(pieces)))
    } else {
        Composed::Plain(
            pieces
                .into_iter()
                .map(|piece| match piece {
                    TextPiece::Literal(s) => s,
                    _ => String::new(),
                })
                .collect(),
        )
    }
}

fn carries_secret(piece: &TextPiece) -> bool {
    !matches!(piece, TextPiece::Literal(_))
}

fn merge_literals(pieces: Vec<TextPiece>) -> Vec<TextPiece> {
    let mut merged: Vec<TextPiece> = Vec::with_capacity(pieces.len());
    for piece in pieces {
        match (piece, merged.last_mut()) {
            (TextPiece::Literal(s), _) if s.is_empty() => {}
            (TextPiece::Literal(s), Some(TextPiece::Literal(prior))) => prior.push_str(&s),
            (piece, _) => merged.push(piece),
        }
    }
    merged
}

pub fn reify(text: &Text) -> Option<SecretText> {
    let pieces: Vec<TextPiece> = text
        .chunks()
        .map(|chunk| match chunk {
            Chunk::Lit(s) => TextPiece::Literal(s.clone()),
            Chunk::Hole(secret) => TextPiece::Sealed(secret.clone()),
        })
        .collect();
    match compose(pieces) {
        Composed::Tainted(text) => Some(text),
        Composed::Plain(_) => None,
    }
}

thread_local! {
    static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) };
    static FAILURE: RefCell<Option<(String, Range<usize>)>> = const { RefCell::new(None) };
}

pub fn note_failure(msg: String, span: Range<usize>) {
    FAILURE.with(|cell| {
        let mut noted = cell.borrow_mut();
        if noted.is_none() {
            *noted = Some((msg, span));
        }
    });
}

pub fn note_inspection(what: &str) {
    note_failure(
        format!(
            "a secret cannot be {what} — only transforming it into other text, or joining it \
             into a larger string, is defined on a sealed value"
        ),
        0..0,
    );
}

pub fn take_failure() -> Option<(String, Range<usize>)> {
    FAILURE.with(|cell| cell.borrow_mut().take())
}

struct Session {
    entry: PathBuf,
    options: SecretOptions,
    key: Option<FleetKey>,
    provider: Option<Provider>,
}

pub fn with_session<T>(entry: &Path, options: SecretOptions, work: impl FnOnce() -> T) -> T {
    let prior = SESSION.with(|cell| {
        cell.replace(Some(Session {
            entry: entry.to_path_buf(),
            options,
            key: None,
            provider: None,
        }))
    });
    let result = work();
    SESSION.with(|cell| cell.replace(prior));
    result
}

pub fn resolve(key: &str) -> Result<ResolvedSecret, String> {
    SESSION.with(|cell| {
        let mut borrowed = cell.borrow_mut();
        let session = borrowed.as_mut().ok_or_else(|| compiled_from_string(key))?;
        if session.key.is_none() {
            session.key = Some(FleetKey::read(session.options.key_file.as_deref())?);
        }
        if session.provider.is_none() {
            session.provider = Some(Provider::open(&session.entry, &session.options)?);
        }
        let provider = session.provider.as_ref().expect("provider just opened");
        provider.value(key).map(|plaintext| ResolvedSecret {
            key: key.to_string(),
            plaintext,
        })
    })
}

pub fn seal(plaintext: &str) -> Result<Secret, String> {
    SESSION.with(
        |cell| match cell.borrow().as_ref().and_then(|s| s.key.as_ref()) {
            Some(key) => key.seal(plaintext),
            None => {
                Err("no fleet secret key is loaded, so this value cannot be sealed".to_string())
            }
        },
    )
}

fn compiled_from_string(key: &str) -> String {
    format!(
        "`{GET} \"{key}\"` needs a source file to search for `{MANIFEST_FILE}` from, \
         but this program was compiled from a string rather than a file"
    )
}

struct FleetKey {
    key_id: String,
    cipher: Aes256SivAead,
}

impl FleetKey {
    fn read(path: Option<&Path>) -> Result<FleetKey, String> {
        let path = path.ok_or_else(|| {
            "no fleet secret key is configured — pass `--secret-key <FILE>` or set \
             `GOLEM_SECRET_KEY_FILE`"
                .to_string()
        })?;
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read the fleet secret key {}: {e}", path.display()))?;
        let bytes = hex::decode(text.trim()).map_err(|_| malformed_key(path))?;
        if bytes.len() != KEY_BYTES {
            return Err(malformed_key(path));
        }
        let cipher = Aes256SivAead::new_from_slice(&bytes).map_err(|_| malformed_key(path))?;
        Ok(FleetKey {
            key_id: hex::encode(&blake3::hash(&bytes).as_bytes()[..8]),
            cipher,
        })
    }

    fn seal(&self, plaintext: &str) -> Result<Secret, String> {
        let ciphertext = self
            .cipher
            .encrypt(&Nonce::default(), plaintext.as_bytes())
            .map_err(|_| "the fleet secret key could not seal a value".to_string())?;
        Ok(Secret::Sealed {
            key_id: self.key_id.clone(),
            ciphertext,
        })
    }
}

fn malformed_key(path: &Path) -> String {
    format!(
        "the fleet secret key {} must be {} hexadecimal characters (a {KEY_BYTES}-byte AES-SIV key)",
        path.display(),
        KEY_BYTES * 2
    )
}

struct Provider {
    manifest: PathBuf,
    name: String,
    values: BTreeMap<String, String>,
    declared: Vec<String>,
}

impl Provider {
    fn open(entry: &Path, options: &SecretOptions) -> Result<Provider, String> {
        let manifest = manifest_above(entry).ok_or_else(|| no_manifest(entry))?;
        let mut secrets = secretspec::Secrets::load_from(&manifest)
            .map_err(|e| format!("cannot load {}: {e}", manifest.display()))?;
        if std::env::var_os("SECRETSPEC_REASON").is_none() {
            secrets = secrets.with_reason(format!(
                "emetc is sealing this secret into the golem manifest compiled from {}",
                entry.display()
            ));
        }
        if let Some(provider) = &options.provider {
            secrets.set_provider(provider.clone());
        }
        if let Some(profile) = &options.profile {
            secrets.set_profile(profile.clone());
        }
        let report = secrets.report().map_err(|e| {
            format!(
                "cannot read the secrets declared in {}: {e}",
                manifest.display()
            )
        })?;
        let declared = report.secrets.iter().map(|s| s.name.clone()).collect();
        let values = match secrets.resolve() {
            Ok(resolved) => resolved
                .secrets
                .into_iter()
                .filter_map(|(name, secret)| secret.value.map(|value| (name, value)))
                .collect(),
            Err(_) => BTreeMap::new(),
        };
        Ok(Provider {
            manifest,
            name: report.provider,
            values,
            declared,
        })
    }

    fn value(&self, key: &str) -> Result<String, String> {
        if let Some(value) = self.values.get(key) {
            return Ok(value.clone());
        }
        if self.declared.iter().any(|name| name == key) {
            return Err(format!(
                "secret `{key}` is declared in {} but provider `{}` has no value for it",
                self.manifest.display(),
                self.name
            ));
        }
        Err(format!(
            "secret `{key}` is not declared in {} — declared secrets are: {}",
            self.manifest.display(),
            if self.declared.is_empty() {
                "(none)".to_string()
            } else {
                self.declared.join(", ")
            }
        ))
    }
}

fn no_manifest(entry: &Path) -> String {
    format!(
        "no `{MANIFEST_FILE}` found in {} or any parent directory",
        entry.parent().unwrap_or(Path::new(".")).display()
    )
}

fn manifest_above(entry: &Path) -> Option<PathBuf> {
    let mut dir = entry.parent()?;
    loop {
        let candidate = dir.join(MANIFEST_FILE);
        if candidate.is_file() {
            return Some(candidate);
        }
        dir = dir.parent()?;
    }
}
