use scroll_format::{
    content_id, content_id_of_glyph, from_bytes, to_bytes, ContentId, FromBytesError, Glyph,
    Manifest, Scroll, FORMAT_VERSION,
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
            Glyph::File {
                path: "/etc/nginx/nginx.conf".to_string(),
                contents: "worker_processes auto;".to_string(),
                mode: "0644".to_string(),
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
    0x03, 0x77, 0x65, 0x62, 0x04, 0x00, 0x05, 0x6e, 0x67, 0x69, 0x6e, 0x78, 0x01, 0x0d, 0x6e, 0x67,
    0x69, 0x6e, 0x78, 0x2e, 0x73, 0x65, 0x72, 0x76, 0x69, 0x63, 0x65, 0x02, 0x15, 0x2f, 0x65, 0x74,
    0x63, 0x2f, 0x6e, 0x67, 0x69, 0x6e, 0x78, 0x2f, 0x6e, 0x67, 0x69, 0x6e, 0x78, 0x2e, 0x63, 0x6f,
    0x6e, 0x66, 0x16, 0x77, 0x6f, 0x72, 0x6b, 0x65, 0x72, 0x5f, 0x70, 0x72, 0x6f, 0x63, 0x65, 0x73,
    0x73, 0x65, 0x73, 0x20, 0x61, 0x75, 0x74, 0x6f, 0x3b, 0x04, 0x30, 0x36, 0x34, 0x34, 0x03, 0x0a,
    0x2f, 0x65, 0x74, 0x63, 0x2f, 0x68, 0x6f, 0x73, 0x74, 0x73, 0x13, 0x31, 0x32, 0x37, 0x2e, 0x30,
    0x2e, 0x30, 0x2e, 0x31, 0x20, 0x6c, 0x6f, 0x63, 0x61, 0x6c, 0x68, 0x6f, 0x73, 0x74,
];

const GOLDEN_CONTENT_ID: &str =
    "cf389a4c5d022d32d97d1ca50a85ac88c3efe55a5666632a778704e92b2152ba";

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
