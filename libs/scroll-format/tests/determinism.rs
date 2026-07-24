use scroll_format::{
    content_id, content_id_of_glyph, from_bytes, to_bytes, ContentId, Entry, FromBytesError, Glyph,
    Manifest, Perms, Scroll, FORMAT_VERSION,
};

fn fixed_scroll() -> Scroll {
    Scroll {
        name: "web".to_string(),
        glyphs: vec![
            Glyph::AptPackage {
                name: "nginx".to_string(),
            },
            Glyph::SystemdService {
                unit: "nginx.service".to_string(),
            },
            Glyph::Filesystem {
                path: "/etc/nginx/nginx.conf".to_string(),
                entry: Entry::File {
                    contents: "worker_processes auto;".to_string(),
                    perms: Perms {
                        mode: 0o644,
                        owner: None,
                        group: None,
                    },
                },
            },
            Glyph::LineInFile {
                path: "/etc/hosts".to_string(),
                line: "127.0.0.1 localhost".to_string(),
            },
        ],
    }
}

fn other_scroll() -> Scroll {
    Scroll {
        name: "db".to_string(),
        glyphs: vec![Glyph::AptPackage {
            name: "postgresql".to_string(),
        }],
    }
}

const GOLDEN_SCROLL_BYTES: &[u8] = &[
    3, 119, 101, 98, 4, 0, 5, 110, 103, 105, 110, 120, 1, 13, 110, 103, 105, 110, 120, 46, 115,
    101, 114, 118, 105, 99, 101, 2, 21, 47, 101, 116, 99, 47, 110, 103, 105, 110, 120, 47, 110,
    103, 105, 110, 120, 46, 99, 111, 110, 102, 0, 22, 119, 111, 114, 107, 101, 114, 95, 112, 114,
    111, 99, 101, 115, 115, 101, 115, 32, 97, 117, 116, 111, 59, 164, 3, 0, 0, 3, 10, 47, 101, 116,
    99, 47, 104, 111, 115, 116, 115, 19, 49, 50, 55, 46, 48, 46, 48, 46, 49, 32, 108, 111, 99, 97,
    108, 104, 111, 115, 116,
];

const GOLDEN_CONTENT_ID: &str =
    "39b8ce7553c9131954e44e0532745c03aabc27b014208a3bdea66986f1aa7df9";

#[test]
fn fixed_scroll_serializes_to_golden_bytes() {
    let bytes = postcard::to_stdvec(&fixed_scroll()).unwrap();
    assert_eq!(bytes, GOLDEN_SCROLL_BYTES);
}

#[test]
fn fixed_scroll_hashes_to_constant_content_id() {
    let id = content_id(&fixed_scroll());
    assert_eq!(id.to_string(), GOLDEN_CONTENT_ID);
}

#[test]
fn fixed_glyph_hashes_to_constant_content_id() {
    let glyph = Glyph::AptPackage {
        name: "nginx".to_string(),
    };
    let id = content_id_of_glyph(&glyph);
    assert_eq!(id, content_id_of_glyph(&glyph));
    let expected = {
        let bytes = postcard::to_stdvec(&glyph).unwrap();
        ContentId(*blake3::hash(&bytes).as_bytes())
    };
    assert_eq!(id, expected);
}

#[test]
fn glyph_content_id_is_invariant_to_surrounding_scroll() {
    let glyph = Glyph::AptPackage {
        name: "nginx".to_string(),
    };
    let from_fixed = fixed_scroll()
        .glyphs
        .iter()
        .find(|g| **g == glyph)
        .map(content_id_of_glyph)
        .unwrap();
    assert_eq!(content_id_of_glyph(&glyph), from_fixed);
}

#[test]
fn content_id_string_round_trips() {
    let id = content_id(&fixed_scroll());
    let parsed: ContentId = id.to_string().parse().unwrap();
    assert_eq!(parsed, id);
}

#[test]
fn manifest_round_trips_through_bytes() {
    let manifest = Manifest::from_scrolls(vec![fixed_scroll(), other_scroll()], "0.1.0");
    let decoded = from_bytes(&to_bytes(&manifest)).unwrap();
    assert_eq!(decoded, manifest);
}

#[test]
fn content_id_is_invariant_to_manifest_position() {
    let first = Manifest::from_scrolls(vec![fixed_scroll(), other_scroll()], "0.1.0");
    let second = Manifest::from_scrolls(vec![other_scroll(), fixed_scroll()], "0.1.0");

    let from_first = &first.scrolls[0].content_id;
    let from_second = &second.scrolls[1].content_id;

    assert_eq!(from_first, from_second);
}

#[test]
fn content_id_is_invariant_to_emet_version() {
    let a = Manifest::from_scrolls(vec![fixed_scroll()], "0.1.0");
    let b = Manifest::from_scrolls(vec![fixed_scroll()], "9.9.9-nightly");

    assert_eq!(a.scrolls[0].content_id, b.scrolls[0].content_id);
}

#[test]
fn unknown_format_version_is_a_clean_error() {
    let mut manifest = Manifest::from_scrolls(vec![fixed_scroll()], "0.1.0");
    manifest.format_version = FORMAT_VERSION + 1;
    let bytes = to_bytes(&manifest);

    match from_bytes(&bytes) {
        Err(FromBytesError::UnsupportedFormatVersion { found, supported }) => {
            assert_eq!(found, FORMAT_VERSION + 1);
            assert_eq!(supported, FORMAT_VERSION);
        }
        other => panic!("expected UnsupportedFormatVersion, got {other:?}"),
    }
}

const GOLDEN_MANIFEST_BYTES: &[u8] = &[
    2, 5, 48, 46, 49, 46, 48, 2, 57, 184, 206, 117, 83, 201, 19, 25, 84, 228, 78, 5, 50, 116, 92, 3,
    170, 188, 39, 176, 20, 32, 138, 59, 222, 166, 105, 134, 241, 170, 125, 249, 3, 119, 101, 98, 4,
    0, 5, 110, 103, 105, 110, 120, 1, 13, 110, 103, 105, 110, 120, 46, 115, 101, 114, 118, 105, 99,
    101, 2, 21, 47, 101, 116, 99, 47, 110, 103, 105, 110, 120, 47, 110, 103, 105, 110, 120, 46, 99,
    111, 110, 102, 0, 22, 119, 111, 114, 107, 101, 114, 95, 112, 114, 111, 99, 101, 115, 115, 101,
    115, 32, 97, 117, 116, 111, 59, 164, 3, 0, 0, 3, 10, 47, 101, 116, 99, 47, 104, 111, 115, 116,
    115, 19, 49, 50, 55, 46, 48, 46, 48, 46, 49, 32, 108, 111, 99, 97, 108, 104, 111, 115, 116, 202,
    232, 188, 199, 207, 173, 171, 127, 82, 92, 189, 36, 123, 80, 218, 4, 178, 204, 164, 177, 27, 34,
    88, 45, 144, 29, 52, 139, 219, 251, 135, 25, 2, 100, 98, 1, 0, 10, 112, 111, 115, 116, 103, 114,
    101, 115, 113, 108,
];

// A tripwire around `ContentId`'s hand-written serde (see `content_id.rs`). The
// scheme rests on one assumption — postcard reports `is_human_readable() ==
// false` — and one guarantee — the postcard wire bytes never move without a
// `format_version` bump (ADR 0012). These four tests fail loudly if either
// breaks:
//   - `postcard_serializer_is_not_human_readable` checks the assumption itself.
//   - `content_id_postcard_encoding_is_thirty_two_raw_bytes` checks a lone id
//     still encodes as the 32 raw digest bytes, not a hex string.
//   - `manifest_postcard_bytes_are_unchanged_by_content_id_serde` compares a
//     whole manifest against golden bytes; any drift is a silent wire change.
//   - `content_id_serializes_to_hex_string_in_json_and_parses_back` checks the
//     human-readable branch round-trips through hex.
#[test]
fn postcard_serializer_is_not_human_readable() {
    use serde::Serializer as _;
    let mut serializer = postcard::Serializer {
        output: postcard::ser_flavors::StdVec::new(),
    };
    assert!(
        !(&mut serializer).is_human_readable(),
        "postcard's Serializer must report is_human_readable() == false; \
         ContentId's hand-written impl branches on this to keep postcard bytes as raw digest"
    );
}

#[test]
fn content_id_postcard_encoding_is_thirty_two_raw_bytes() {
    let cid = ContentId([7u8; 32]);
    let bytes = postcard::to_stdvec(&cid).unwrap();
    assert_eq!(
        bytes.len(),
        32,
        "a ContentId must postcard-encode as exactly 32 raw digest bytes, not a hex string; \
         if this fails the is_human_readable() assumption is wrong and the wire format changed"
    );
    assert_eq!(bytes, [7u8; 32]);
    let round_tripped: ContentId = postcard::from_bytes(&bytes).unwrap();
    assert_eq!(round_tripped, cid);
}

#[test]
fn manifest_postcard_bytes_are_unchanged_by_content_id_serde() {
    let manifest = Manifest::from_scrolls(vec![fixed_scroll(), other_scroll()], "0.1.0");
    let bytes = to_bytes(&manifest);
    assert_eq!(
        bytes, GOLDEN_MANIFEST_BYTES,
        "the postcard manifest bytes (content-ids included) must be byte-for-byte unchanged; \
         a difference is a format_version bump per ADR 0012"
    );
    assert_eq!(from_bytes(&bytes).unwrap(), manifest);
}

#[test]
fn content_id_serializes_to_hex_string_in_json_and_parses_back() {
    let cid = content_id(&fixed_scroll());
    let json = serde_json::to_string(&cid).unwrap();
    assert_eq!(json, format!("\"{GOLDEN_CONTENT_ID}\""));
    assert_eq!(json.len(), 64 + 2);
    let parsed: ContentId = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, cid);
}
