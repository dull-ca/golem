use std::path::{Path, PathBuf};
use std::sync::Arc;

use golemd::config::RetryConfig;
use golemd::fake_reconciler::FakeReconciler;
use golemd::foreman::Foreman;
use golemd::journal::Inverse;
use golemd::planroom::{MemoryPlanRoom, SqlitePlanRoom};
use golemd::reconcilers::HostReconciler;
use golemd::report::{FailClassReport, TopOutcome};
use golemd::secrets::Keyring;
use scroll_format::{
    to_bytes, Chunk, Contents, Entry, Glyph, Manifest, Perms, Scroll, Secret, Text,
};

/// The fleet key `apps/emet/tests/secrets.rs` seals with, and the one
/// `SEALED_BY_EMETC` below was compiled against.
const FLEET_KEY: &str = "00112233445566778899aabbccddeeff\
                         00112233445566778899aabbccddeeff\
                         ffeeddccbbaa99887766554433221100\
                         ffeeddccbbaa99887766554433221100";

const FLEET_KEY_ID: &str = "6fb6c6005355abf3";

const OTHER_FLEET_KEY: &str = "ffeeddccbbaa99887766554433221100\
                               ffeeddccbbaa99887766554433221100\
                               00112233445566778899aabbccddeeff\
                               00112233445566778899aabbccddeeff";

const SECRET_PLAINTEXT: &str = "hunter2-correct-horse";

/// A real `emetc` output, byte for byte: `file { contents = "password=${pw}\n" }`
/// and `lineInFile { line = "token=${pw}" }` over
/// `pw = Secretspec.get "DB_PASSWORD"`, sealed to [`FLEET_KEY`] with the dotenv
/// provider supplying [`SECRET_PLAINTEXT`]. Pinned as bytes so a change to
/// either side of the seal — `emetc`'s cipher, nonce, key-file format, or
/// `key_id` derivation — fails here rather than on a host.
const SEALED_BY_EMETC: &[u8] = &[
    5, 5, 48, 46, 49, 46, 48, 1, 174, 244, 43, 126, 213, 33, 18, 13, 60, 186, 38, 87, 246, 159, 2,
    26, 77, 230, 150, 254, 239, 39, 139, 147, 122, 31, 188, 255, 120, 52, 231, 118, 5, 115, 99, 97,
    108, 121, 0, 0, 0, 2, 2, 27, 47, 101, 116, 99, 47, 103, 111, 108, 101, 109, 45, 102, 105, 120,
    116, 117, 114, 101, 47, 97, 112, 112, 46, 99, 111, 110, 102, 0, 1, 3, 0, 9, 112, 97, 115, 115,
    119, 111, 114, 100, 61, 1, 0, 16, 54, 102, 98, 54, 99, 54, 48, 48, 53, 51, 53, 53, 97, 98, 102,
    51, 37, 11, 165, 174, 218, 187, 220, 68, 113, 44, 214, 116, 178, 49, 132, 127, 237, 143, 234,
    122, 202, 172, 149, 58, 84, 196, 34, 53, 239, 169, 102, 61, 241, 147, 174, 144, 15, 64, 0, 1,
    10, 128, 3, 0, 0, 3, 29, 47, 101, 116, 99, 47, 103, 111, 108, 101, 109, 45, 102, 105, 120, 116,
    117, 114, 101, 47, 101, 120, 116, 114, 97, 46, 99, 111, 110, 102, 1, 2, 0, 6, 116, 111, 107,
    101, 110, 61, 1, 0, 16, 54, 102, 98, 54, 99, 54, 48, 48, 53, 51, 53, 53, 97, 98, 102, 51, 37,
    11, 165, 174, 218, 187, 220, 68, 113, 44, 214, 116, 178, 49, 132, 127, 237, 143, 234, 122, 202,
    172, 149, 58, 84, 196, 34, 53, 239, 169, 102, 61, 241, 147, 174, 144, 15, 64,
];

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(tag: &str) -> Fixture {
        let root = std::env::temp_dir().join(format!(
            "golemd_secrets_{tag}_{}_{}",
            std::process::id(),
            fastrand::u64(..)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        Fixture { root }
    }

    fn key_file(&self, hex: &str) -> PathBuf {
        let path = self.root.join("fleet.key");
        std::fs::write(&path, hex).unwrap();
        path
    }

    fn path(&self, rel: &str) -> String {
        self.root.join(rel).to_str().unwrap().to_string()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn seal(plaintext: &str, key_hex: &str) -> Secret {
    use aes_siv::aead::{Aead, KeyInit};
    use aes_siv::{Aes256SivAead, Nonce};
    let bytes = hex::decode(key_hex).unwrap();
    let cipher = Aes256SivAead::new_from_slice(&bytes).unwrap();
    Secret::Sealed {
        key_id: hex::encode(&blake3::hash(&bytes).as_bytes()[..8]),
        ciphertext: cipher
            .encrypt(&Nonce::default(), plaintext.as_bytes())
            .unwrap(),
    }
}

fn perms() -> Perms {
    Perms {
        mode: 0o600,
        owner: None,
        group: None,
    }
}

fn file_glyph(path: &str, contents: Text) -> Glyph {
    Glyph::Filesystem {
        path: path.to_string(),
        entry: Entry::File {
            contents,
            perms: perms(),
        },
    }
}

fn manifest(glyphs: Vec<Glyph>) -> Vec<u8> {
    to_bytes(&Manifest::from_scrolls(
        vec![Scroll {
            name: "scaly".to_string(),
            policy: None,
            notifies: Vec::new(),
            contents: Contents::Glyphs(glyphs),
        }],
        "0.1.0",
    ))
}

fn quiet_retry() -> RetryConfig {
    RetryConfig {
        max_attempts: 1,
        base_delay_ms: 0,
        ..Default::default()
    }
}

fn host_foreman(key_file: Option<&Path>) -> Arc<Foreman> {
    Arc::new(
        Foreman::new(
            "scaly".to_string(),
            Box::new(MemoryPlanRoom::new()),
            Box::new(HostReconciler::system().with_keyring(keyring(key_file))),
        )
        .with_retry_config(quiet_retry()),
    )
}

fn fake_foreman(key_file: Option<&Path>) -> Arc<Foreman> {
    Arc::new(
        Foreman::new(
            "scaly".to_string(),
            Box::new(MemoryPlanRoom::new()),
            Box::new(FakeReconciler::new().with_keyring(keyring(key_file))),
        )
        .with_retry_config(quiet_retry()),
    )
}

fn keyring(key_file: Option<&Path>) -> Keyring {
    match key_file {
        Some(path) => Keyring::from_key_file(path).unwrap(),
        None => Keyring::without_key(),
    }
}

#[test]
fn a_sealed_file_reaches_the_host_as_plaintext() {
    let fixture = Fixture::new("roundtrip");
    let key = fixture.key_file(FLEET_KEY);
    let target = fixture.path("app.conf");
    let foreman = host_foreman(Some(&key));

    let report = foreman
        .apply_manifest(&manifest(vec![file_glyph(
            &target,
            Text::composed(vec![
                Chunk::Lit("password=".into()),
                Chunk::Hole(seal(SECRET_PLAINTEXT, FLEET_KEY)),
                Chunk::Lit("\n".into()),
            ]),
        )]))
        .unwrap();

    assert_eq!(report.outcome, TopOutcome::Settled, "{report:?}");
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        format!("password={SECRET_PLAINTEXT}\n")
    );
}

#[test]
fn a_manifest_emetc_sealed_opens_under_the_same_key_file() {
    let fixture = Fixture::new("interop");
    let key = fixture.key_file(FLEET_KEY);
    let keyring = Keyring::from_key_file(&key).unwrap();
    let manifest = scroll_format::from_bytes(SEALED_BY_EMETC).unwrap();

    let mut opened = Vec::new();
    for glyph in manifest.scrolls[0].scroll.all_glyphs() {
        let text = match &glyph {
            Glyph::Filesystem {
                entry: Entry::File { contents, .. },
                ..
            } => contents,
            Glyph::LineInFile { line, .. } => line,
            other => panic!("unexpected glyph {other:?}"),
        };
        opened.push(keyring.open(text, &glyph.key()).unwrap().into_owned());
    }

    assert_eq!(
        opened,
        vec![
            format!("password={SECRET_PLAINTEXT}\n"),
            format!("token={SECRET_PLAINTEXT}"),
        ]
    );
    assert!(
        String::from_utf8_lossy(SEALED_BY_EMETC).contains(FLEET_KEY_ID),
        "the fixture is sealed under the key id this test's key file derives"
    );
}

#[test]
fn a_sealed_value_with_no_key_configured_is_a_fatal_failure_naming_the_flag() {
    let fixture = Fixture::new("nokey");
    let foreman = fake_foreman(None);

    let report = foreman
        .apply_manifest(&manifest(vec![file_glyph(
            &fixture.path("app.conf"),
            Text::composed(vec![Chunk::Hole(seal(SECRET_PLAINTEXT, FLEET_KEY))]),
        )]))
        .unwrap();

    let failure = &report.units[0].failures[0];
    assert_eq!(failure.class, FailClassReport::Fatal);
    assert!(
        failure.message.contains("--secrets-key-file")
            && failure.message.contains("[secrets] key_file"),
        "the failure names both ways to configure the key: {}",
        failure.message
    );
    assert!(!failure.message.contains(SECRET_PLAINTEXT));
}

#[test]
fn a_sealed_value_under_another_key_is_a_fatal_failure_naming_both_key_ids() {
    let fixture = Fixture::new("wrongkey");
    let key = fixture.key_file(OTHER_FLEET_KEY);
    let foreman = fake_foreman(Some(&key));

    let report = foreman
        .apply_manifest(&manifest(vec![file_glyph(
            &fixture.path("app.conf"),
            Text::composed(vec![Chunk::Hole(seal(SECRET_PLAINTEXT, FLEET_KEY))]),
        )]))
        .unwrap();

    let failure = &report.units[0].failures[0];
    assert_eq!(failure.class, FailClassReport::Fatal);
    assert!(
        failure.message.contains(FLEET_KEY_ID),
        "the failure names the key the value was sealed under: {}",
        failure.message
    );
    assert!(
        failure
            .message
            .contains(Keyring::from_key_file(&key).unwrap().key_id().unwrap()),
        "the failure names the key this host holds: {}",
        failure.message
    );
}

#[test]
fn a_reference_is_refused_naming_its_provider_and_key() {
    let fixture = Fixture::new("reference");
    let key = fixture.key_file(FLEET_KEY);
    let foreman = fake_foreman(Some(&key));

    let report = foreman
        .apply_manifest(&manifest(vec![file_glyph(
            &fixture.path("app.conf"),
            Text::composed(vec![Chunk::Hole(Secret::Reference {
                provider: "onepassword".into(),
                key: "DB_PASSWORD".into(),
            })]),
        )]))
        .unwrap();

    let failure = &report.units[0].failures[0];
    assert_eq!(failure.class, FailClassReport::Fatal);
    assert!(
        failure.message.contains("onepassword") && failure.message.contains("DB_PASSWORD"),
        "the refusal names the provider and the key: {}",
        failure.message
    );
    assert!(
        failure.message.contains("host-side"),
        "the refusal says host-side resolution is unbuilt: {}",
        failure.message
    );
}

#[test]
fn a_corrupt_ciphertext_is_a_fatal_failure_not_a_panic_and_writes_nothing() {
    let fixture = Fixture::new("corrupt");
    let key = fixture.key_file(FLEET_KEY);
    let target = fixture.path("app.conf");
    let foreman = host_foreman(Some(&key));

    let report = foreman
        .apply_manifest(&manifest(vec![file_glyph(
            &target,
            Text::composed(vec![Chunk::Hole(Secret::Sealed {
                key_id: FLEET_KEY_ID.to_string(),
                ciphertext: vec![0; 32],
            })]),
        )]))
        .unwrap();

    assert_eq!(report.units[0].failures[0].class, FailClassReport::Fatal);
    assert!(
        !Path::new(&target).exists(),
        "a glyph that cannot be unsealed never reaches the filesystem"
    );
}

#[test]
fn a_manifest_with_no_secret_applies_with_no_key_configured() {
    let fixture = Fixture::new("nosecret");
    let target = fixture.path("plain.conf");
    let foreman = host_foreman(None);

    let report = foreman
        .apply_manifest(&manifest(vec![file_glyph(&target, "just text\n".into())]))
        .unwrap();

    assert_eq!(report.outcome, TopOutcome::Settled, "{report:?}");
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "just text\n");
}

#[test]
fn no_plaintext_reaches_the_report_the_events_or_the_plan() {
    let fixture = Fixture::new("redaction");
    let key = fixture.key_file(FLEET_KEY);
    let target = fixture.path("app.conf");
    let line_target = fixture.path("extra.conf");
    let foreman = host_foreman(Some(&key));

    let bytes = manifest(vec![
        file_glyph(
            &target,
            Text::composed(vec![
                Chunk::Lit("password=".into()),
                Chunk::Hole(seal(SECRET_PLAINTEXT, FLEET_KEY)),
                Chunk::Lit("\n".into()),
            ]),
        ),
        Glyph::LineInFile {
            path: line_target.clone(),
            line: Text::composed(vec![
                Chunk::Lit("token=".into()),
                Chunk::Hole(seal(SECRET_PLAINTEXT, FLEET_KEY)),
            ]),
        },
    ]);

    let plan = foreman.plan_manifest(&bytes).unwrap();
    let rendered_plan = serde_json::to_string(&plan).unwrap();
    assert!(
        !rendered_plan.contains(SECRET_PLAINTEXT),
        "the plan render leaked the plaintext: {rendered_plan}"
    );
    assert!(
        rendered_plan.contains("<sealed"),
        "the plan still shows a sealed hole where the secret is: {rendered_plan}"
    );

    let report = foreman.apply_manifest(&bytes).unwrap();
    assert_eq!(report.outcome, TopOutcome::Settled, "{report:?}");
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        format!("password={SECRET_PLAINTEXT}\n"),
        "the host really did get the plaintext — otherwise this test proves nothing"
    );

    let rendered_report = serde_json::to_string(&report).unwrap();
    assert!(
        !rendered_report.contains(SECRET_PLAINTEXT),
        "the reconcile report leaked the plaintext: {rendered_report}"
    );

    let rendered_history = serde_json::to_string(&foreman.revisions().unwrap()).unwrap();
    assert!(
        !rendered_history.contains(SECRET_PLAINTEXT),
        "`GET /revisions` — what `golemctl history` and `show` print — leaked the plaintext: \
         {rendered_history}"
    );

    let rendered_state = serde_json::to_string(&foreman.applied_state().unwrap()).unwrap();
    assert!(
        !rendered_state.contains(SECRET_PLAINTEXT),
        "`GET /state` leaked the plaintext: {rendered_state}"
    );

    let reconcile_id = foreman.latest_reconcile_id().unwrap().unwrap();
    let projection = foreman.progress_projection(reconcile_id, 0).unwrap();
    let rendered_events = serde_json::to_string(&projection).unwrap();
    assert!(
        !rendered_events.contains(SECRET_PLAINTEXT),
        "a progress event or the live apply tree leaked the plaintext: {rendered_events}"
    );
}

#[test]
fn a_failed_write_of_a_sealed_file_reports_no_plaintext() {
    let fixture = Fixture::new("failedwrite");
    let key = fixture.key_file(FLEET_KEY);
    let blocked = fixture.path("blocked");
    std::fs::write(&blocked, "not a directory").unwrap();
    let foreman = host_foreman(Some(&key));

    let report = foreman
        .apply_manifest(&manifest(vec![file_glyph(
            &format!("{blocked}/app.conf"),
            Text::composed(vec![
                Chunk::Lit("password=".into()),
                Chunk::Hole(seal(SECRET_PLAINTEXT, FLEET_KEY)),
            ]),
        )]))
        .unwrap();

    assert_ne!(report.outcome, TopOutcome::Settled, "the write must fail");
    let rendered = serde_json::to_string(&report).unwrap();
    assert!(
        !rendered.contains(SECRET_PLAINTEXT),
        "a FAILED file write leaked the contents it tried to write: {rendered}"
    );
}

#[test]
fn a_sealed_line_is_journalled_sealed_and_reverses_from_the_ciphertext() {
    let fixture = Fixture::new("journal");
    let key = fixture.key_file(FLEET_KEY);
    let target = fixture.path("extra.conf");
    let foreman = host_foreman(Some(&key));
    let line = Text::composed(vec![
        Chunk::Lit("token=".into()),
        Chunk::Hole(seal(SECRET_PLAINTEXT, FLEET_KEY)),
    ]);

    let report = foreman
        .apply_manifest(&manifest(vec![Glyph::LineInFile {
            path: target.clone(),
            line: line.clone(),
        }]))
        .unwrap();

    let journalled = report
        .revision
        .outcomes
        .iter()
        .find_map(|o| match &o.inverse {
            Inverse::RemoveLineInFile { line, .. } => Some(line.clone()),
            _ => None,
        })
        .expect("the line-in-file apply recorded its inverse");
    assert_eq!(journalled, line);
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        format!("token={SECRET_PLAINTEXT}\n")
    );

    foreman.apply_manifest(&manifest(vec![])).unwrap();
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        "",
        "reverse unsealed the journalled ciphertext to find the line it added"
    );
}

/// ADR 0015 makes a `file` inverse hold the **prior host contents** inline so
/// removal stays exact, and those contents are read back off the disk golem
/// itself wrote plaintext to. Rotating a secret is therefore the moment the
/// previous credential would become a durable plaintext copy in `planroom.db`,
/// served by `GET /revisions` and `GET /state`. It is sealed instead, and
/// reverse still restores the exact prior bytes.
#[test]
fn a_rotated_secrets_prior_plaintext_is_journalled_sealed_and_still_reverses() {
    let fixture = Fixture::new("priorplaintext");
    let key = fixture.key_file(FLEET_KEY);
    let target = fixture.path("app.conf");
    let foreman = host_foreman(Some(&key));

    for plaintext in [SECRET_PLAINTEXT, "rotated-secret"] {
        foreman
            .apply_manifest(&manifest(vec![file_glyph(
                &target,
                Text::composed(vec![
                    Chunk::Lit("password=".into()),
                    Chunk::Hole(seal(plaintext, FLEET_KEY)),
                ]),
            )]))
            .unwrap();
    }
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        "password=rotated-secret"
    );

    let revisions = foreman.revisions().unwrap();
    let rendered = serde_json::to_string(&revisions).unwrap();
    assert!(
        !rendered.contains(SECRET_PLAINTEXT),
        "the revision journal holds the prior secret in the clear: {rendered}"
    );
    let restored = revisions
        .last()
        .unwrap()
        .outcomes
        .iter()
        .find_map(|o| match &o.inverse {
            Inverse::RestoreFile { contents, .. } => Some(contents.clone()),
            _ => None,
        })
        .expect("the second apply captured the first apply's file as its inverse");
    assert!(matches!(restored, Text::Composed(_)));

    foreman.apply_manifest(&manifest(vec![])).unwrap();
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        format!("password={SECRET_PLAINTEXT}"),
        "reverse unsealed the journalled prior state and restored it exactly"
    );
}

/// The mode golem writes a credential-bearing file at is the mode the *author*
/// declared, and nothing in golemd tightens it. Pinned so the fact is on the
/// record: `mode = "0600"` really does reach disk atomically, without a window
/// at a looser mode, because the perms are set on the temp file before the
/// rename.
#[test]
fn a_sealed_files_mode_is_the_authored_one_and_is_set_before_the_rename() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new("mode");
    let key = fixture.key_file(FLEET_KEY);
    let tight = fixture.path("tight.conf");
    let loose = fixture.path("loose.conf");
    let foreman = host_foreman(Some(&key));
    let secret = || {
        Text::composed(vec![
            Chunk::Lit("password=".into()),
            Chunk::Hole(seal(SECRET_PLAINTEXT, FLEET_KEY)),
        ])
    };

    foreman
        .apply_manifest(&manifest(vec![
            file_glyph(&tight, secret()),
            Glyph::Filesystem {
                path: loose.clone(),
                entry: Entry::File {
                    contents: secret(),
                    perms: Perms {
                        mode: 0o644,
                        owner: None,
                        group: None,
                    },
                },
            },
        ]))
        .unwrap();

    let mode_of = |p: &str| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode_of(&tight), 0o600);
    assert_eq!(
        mode_of(&loose),
        0o644,
        "golemd does not tighten an authored mode, even for a file holding a credential"
    );
}

/// A `lineInFile` glyph has no `mode` field, so a secret-bearing line appended
/// to a file golem has to create lands at the process umask's default — 0644 on
/// a stock host. Pinned as the fact it is: the only glyph kind that can write a
/// credential without the author being able to say how it should be protected.
#[test]
fn a_sealed_line_creating_a_new_file_lands_at_the_umask_default() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new("linemode");
    let key = fixture.key_file(FLEET_KEY);
    let target = fixture.path("created.conf");
    let foreman = host_foreman(Some(&key));

    foreman
        .apply_manifest(&manifest(vec![Glyph::LineInFile {
            path: target.clone(),
            line: Text::composed(vec![
                Chunk::Lit("token=".into()),
                Chunk::Hole(seal(SECRET_PLAINTEXT, FLEET_KEY)),
            ]),
        }]))
        .unwrap();

    let mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        mode & 0o077,
        0o044,
        "the created file is group- and world-readable: {mode:04o}"
    );
}

/// The claim the whole redaction effort rests on, checked against the bytes on
/// disk rather than against a serialization: after applying a secret, rotating
/// it, and tearing the whole scroll down, nothing under the state directory —
/// `planroom.db`, its sqlite sidecars, anything else golemd put there — contains
/// the plaintext.
#[test]
fn nothing_golemd_persists_holds_the_plaintext() {
    let fixture = Fixture::new("atrest");
    let key = fixture.key_file(FLEET_KEY);
    let target = fixture.path("app.conf");
    let line_target = fixture.path("extra.conf");
    let state_dir = fixture.root.join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    let foreman = Arc::new(
        Foreman::new(
            "scaly".to_string(),
            Box::new(SqlitePlanRoom::open(&state_dir.join("planroom.db")).unwrap()),
            Box::new(HostReconciler::system().with_keyring(keyring(Some(&key)))),
        )
        .with_retry_config(quiet_retry()),
    );

    for plaintext in [SECRET_PLAINTEXT, "rotated-secret"] {
        foreman
            .apply_manifest(&manifest(vec![
                file_glyph(
                    &target,
                    Text::composed(vec![
                        Chunk::Lit("password=".into()),
                        Chunk::Hole(seal(plaintext, FLEET_KEY)),
                    ]),
                ),
                Glyph::LineInFile {
                    path: line_target.clone(),
                    line: Text::composed(vec![
                        Chunk::Lit("token=".into()),
                        Chunk::Hole(seal(plaintext, FLEET_KEY)),
                    ]),
                },
            ]))
            .unwrap();
    }
    foreman.apply_manifest(&manifest(vec![])).unwrap();
    drop(foreman);

    let needle = SECRET_PLAINTEXT.as_bytes();
    for entry in std::fs::read_dir(&state_dir).unwrap() {
        let path = entry.unwrap().path();
        let bytes = std::fs::read(&path).unwrap();
        assert!(
            !bytes.windows(needle.len()).any(|w| w == needle),
            "{} holds the secret in the clear",
            path.display()
        );
    }
}

#[test]
fn a_keyless_host_still_journals_a_prior_files_contents_verbatim() {
    let fixture = Fixture::new("keylessprior");
    let target = fixture.path("plain.conf");
    std::fs::write(&target, "was here").unwrap();
    let foreman = host_foreman(None);

    let report = foreman
        .apply_manifest(&manifest(vec![file_glyph(&target, "now this".into())]))
        .unwrap();

    assert_eq!(report.outcome, TopOutcome::Settled, "{report:?}");
    let restored = report
        .revision
        .outcomes
        .iter()
        .find_map(|o| match &o.inverse {
            Inverse::RestoreFile { contents, .. } => Some(contents.clone()),
            _ => None,
        })
        .expect("overwriting a pre-existing file captures it");
    assert_eq!(restored, Text::Plain("was here".into()));

    foreman.apply_manifest(&manifest(vec![])).unwrap();
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "was here");
}
