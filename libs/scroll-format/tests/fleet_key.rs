use scroll_format::{FleetKey, MalformedFleetKey, Secret, UnsealError, FLEET_KEY_BYTES};

const KEY: &str = "00112233445566778899aabbccddeeff\
                   00112233445566778899aabbccddeeff\
                   ffeeddccbbaa99887766554433221100\
                   ffeeddccbbaa99887766554433221100";

const OTHER_KEY: &str = "ffeeddccbbaa99887766554433221100\
                         ffeeddccbbaa99887766554433221100\
                         00112233445566778899aabbccddeeff\
                         00112233445566778899aabbccddeeff";

const PLAINTEXT: &str = "hunter2-correct-horse";

fn parts(secret: &Secret) -> (&str, &[u8]) {
    match secret {
        Secret::Sealed { key_id, ciphertext } => (key_id.as_str(), ciphertext.as_slice()),
        other => panic!("expected a sealed secret, got {other:?}"),
    }
}

/// The bytes are pinned, not computed, so any change to the key length, the
/// `key_id` derivation, the cipher, or the nonce fails here — where it is one
/// diff to read — rather than as a manifest a host cannot open.
#[test]
fn a_known_key_seals_a_known_plaintext_to_known_bytes() {
    let key = FleetKey::from_hex(KEY).unwrap();
    let sealed = key.seal(PLAINTEXT.as_bytes()).unwrap();
    let (key_id, ciphertext) = parts(&sealed);
    assert_eq!(key_id, "6fb6c6005355abf3");
    assert_eq!(
        hex::encode(ciphertext),
        "0ba5aedabbdc44712cd674b231847fed8fea7acaac953a54c42235efa9663df193ae900f40"
    );
}

#[test]
fn the_key_id_is_the_first_eight_bytes_of_the_keys_blake3() {
    let expected = hex::encode(&blake3::hash(&hex::decode(KEY).unwrap()).as_bytes()[..8]);
    assert_eq!(FleetKey::from_hex(KEY).unwrap().key_id(), expected);
}

#[test]
fn sealing_is_deterministic_so_content_addressing_stays_honest() {
    let key = FleetKey::from_hex(KEY).unwrap();
    assert_eq!(
        key.seal(PLAINTEXT.as_bytes()).unwrap(),
        key.seal(PLAINTEXT.as_bytes()).unwrap()
    );
}

#[test]
fn a_sealed_value_unseals_back_to_its_plaintext() {
    let key = FleetKey::from_hex(KEY).unwrap();
    let sealed = key.seal(PLAINTEXT.as_bytes()).unwrap();
    let (key_id, ciphertext) = parts(&sealed);
    assert_eq!(key.unseal(key_id, ciphertext).unwrap(), PLAINTEXT);
}

#[test]
fn a_key_id_from_another_key_is_a_mismatch_rather_than_a_decrypt_attempt() {
    let sealed = FleetKey::from_hex(KEY)
        .unwrap()
        .seal(PLAINTEXT.as_bytes())
        .unwrap();
    let (key_id, ciphertext) = parts(&sealed);
    assert!(matches!(
        FleetKey::from_hex(OTHER_KEY)
            .unwrap()
            .unseal(key_id, ciphertext),
        Err(UnsealError::KeyIdMismatch)
    ));
}

#[test]
fn a_corrupt_ciphertext_is_undecryptable() {
    let key = FleetKey::from_hex(KEY).unwrap();
    assert!(matches!(
        key.unseal(key.key_id(), &[0; 32]),
        Err(UnsealError::Undecryptable)
    ));
}

#[test]
fn ciphertext_that_decrypts_to_non_utf8_bytes_is_refused() {
    let key = FleetKey::from_hex(KEY).unwrap();
    let sealed = key.seal(&[0xff, 0xfe]).unwrap();
    let (key_id, ciphertext) = parts(&sealed);
    assert!(matches!(
        key.unseal(key_id, ciphertext),
        Err(UnsealError::NotUtf8)
    ));
}

#[test]
fn surrounding_whitespace_in_a_key_file_is_not_part_of_the_key() {
    assert_eq!(
        FleetKey::from_hex(&format!("  {KEY}\n")).unwrap().key_id(),
        FleetKey::from_hex(KEY).unwrap().key_id()
    );
}

#[test]
fn a_key_of_the_wrong_length_or_alphabet_is_malformed() {
    assert!(FleetKey::from_hex("0011223344").is_err());
    assert!(FleetKey::from_hex(&format!("{KEY}00")).is_err());
    assert!(FleetKey::from_hex(&"z".repeat(FLEET_KEY_BYTES * 2)).is_err());
}

#[test]
fn the_malformed_key_error_names_the_length_and_the_cipher() {
    assert_eq!(
        MalformedFleetKey.to_string(),
        "must be 128 hexadecimal characters (a 64-byte AES-SIV key)"
    );
}
