use scroll_format::{
    content_id, content_id_of_glyph, from_bytes, to_bytes, ContentId, Contents, Entry,
    FromBytesError, Glyph, Manifest, Perms, Scroll, FORMAT_VERSION,
};

fn fixed_scroll() -> Scroll {
    Scroll {
        name: "web".to_string(),
        policy: None,
        notifies: vec![],
        contents: Contents::Glyphs(vec![
            Glyph::AptPackage {
                name: "nginx".to_string(),
            },
            Glyph::SystemdService {
                unit: "nginx.service".to_string(),
            },
            Glyph::Filesystem {
                path: "/etc/nginx/nginx.conf".to_string(),
                entry: Entry::File {
                    contents: "worker_processes auto;".into(),
                    perms: Perms {
                        mode: 0o644,
                        owner: None,
                        group: None,
                    },
                },
            },
            Glyph::LineInFile {
                path: "/etc/hosts".to_string(),
                line: "127.0.0.1 localhost".into(),
                perms: None,
            },
        ]),
    }
}

fn other_scroll() -> Scroll {
    Scroll {
        name: "db".to_string(),
        policy: None,
        notifies: vec![],
        contents: Contents::Glyphs(vec![Glyph::AptPackage {
            name: "postgresql".to_string(),
        }]),
    }
}

fn notifying_scroll() -> Scroll {
    Scroll {
        notifies: vec!["nginx.service".to_string()],
        ..fixed_scroll()
    }
}

// These golden bytes were deliberately regenerated for the v4 wire shape — the
// recursive `Scroll { name, policy, notifies, contents }` (ADR 0036) — not
// silent drift. A change here without a matching FORMAT_VERSION bump is a bug.
const GOLDEN_SCROLL_BYTES: &[u8] = &[
    3, 119, 101, 98, 0, 0, 0, 4, 0, 5, 110, 103, 105, 110, 120, 1, 13, 110, 103, 105, 110, 120, 46,
    115, 101, 114, 118, 105, 99, 101, 2, 21, 47, 101, 116, 99, 47, 110, 103, 105, 110, 120, 47,
    110, 103, 105, 110, 120, 46, 99, 111, 110, 102, 0, 0, 22, 119, 111, 114, 107, 101, 114, 95,
    112, 114, 111, 99, 101, 115, 115, 101, 115, 32, 97, 117, 116, 111, 59, 164, 3, 0, 0, 3, 10, 47,
    101, 116, 99, 47, 104, 111, 115, 116, 115, 0, 19, 49, 50, 55, 46, 48, 46, 48, 46, 49, 32, 108,
    111, 99, 97, 108, 104, 111, 115, 116, 0,
];

const GOLDEN_CONTENT_ID: &str = "78e50292e06f2c7cfdd7722323315783d25e6f961c6bc5b40067174fc5d1eb74";

// Captured from the v3 code *before* the bump: ADR 0036's "glyph cids are
// untouched" promise written down, which is what makes the first v4 apply a
// Noop pass rather than a Replace storm. A scroll-level change must never move
// these; only a change to a glyph's own shape may.
const VALUELESS_GLYPH_KEYS: &[&str] = &["apt:nginx", "systemd:nginx.service"];

const GLYPH_CONTENT_IDS_BEFORE_V5: &[(&str, &str)] = &[
    (
        "apt:nginx",
        "69cc4176a66accd09e9f786cf5f0b6d38472b0053bc945f3c39b1ca4ac0b9a45",
    ),
    (
        "systemd:nginx.service",
        "4f9c05df286a637c2afbfe3fdcc8db03a49bd58b5501b3af32baf5ed41873fbc",
    ),
    (
        "file:/etc/nginx/nginx.conf",
        "6c9ffe8eb991d71e4b6728d6d3baf32753c427cdbc1568ec929c3fc810a4a9ba",
    ),
    (
        "fileline:/etc/hosts:127.0.0.1 localhost",
        "c56f7c03ccace75906537fc48be527a52c9423463637adee8417459b565c63c0",
    ),
];

// A real v3 artifact, kept as literal bytes because no v4 build can produce
// one — the only way to keep proving that a stale manifest fails cleanly.
const V3_MANIFEST_BYTES: &[u8] = &[
    3, 5, 48, 46, 49, 46, 48, 1, 185, 143, 81, 66, 52, 209, 114, 170, 213, 151, 43, 173, 47, 157,
    227, 170, 217, 192, 190, 150, 18, 112, 45, 120, 136, 87, 37, 221, 178, 134, 97, 240, 3, 119,
    101, 98, 0, 0, 4, 0, 5, 110, 103, 105, 110, 120, 1, 13, 110, 103, 105, 110, 120, 46, 115, 101,
    114, 118, 105, 99, 101, 2, 21, 47, 101, 116, 99, 47, 110, 103, 105, 110, 120, 47, 110, 103,
    105, 110, 120, 46, 99, 111, 110, 102, 0, 22, 119, 111, 114, 107, 101, 114, 95, 112, 114, 111,
    99, 101, 115, 115, 101, 115, 32, 97, 117, 116, 111, 59, 164, 3, 0, 0, 3, 10, 47, 101, 116, 99,
    47, 104, 111, 115, 116, 115, 19, 49, 50, 55, 46, 48, 46, 48, 46, 49, 32, 108, 111, 99, 97, 108,
    104, 111, 115, 116,
];

fn content_id_before_v5(key: &str) -> &'static str {
    GLYPH_CONTENT_IDS_BEFORE_V5
        .iter()
        .find(|(pinned, _)| *pinned == key)
        .map(|(_, cid)| *cid)
        .unwrap_or_else(|| panic!("no pinned pre-v5 content id for {key}"))
}

#[test]
fn a_valueless_glyph_content_id_survived_the_v5_bump() {
    for glyph in fixed_scroll().all_glyphs() {
        if !VALUELESS_GLYPH_KEYS.contains(&glyph.key().as_str()) {
            continue;
        }
        assert_eq!(
            content_id_of_glyph(glyph).to_string(),
            content_id_before_v5(&glyph.key()),
            "{} carries no value and must not move across the bump",
            glyph.key()
        );
    }
}

#[test]
fn a_value_bearing_glyph_content_id_moved_at_the_v5_bump() {
    for glyph in fixed_scroll().all_glyphs() {
        if VALUELESS_GLYPH_KEYS.contains(&glyph.key().as_str()) {
            continue;
        }
        assert_ne!(
            content_id_of_glyph(glyph).to_string(),
            content_id_before_v5(&glyph.key()),
            "{} wraps its value in `Text::Plain`, whose variant tag is a byte the \
             pre-v5 encoding did not have",
            glyph.key()
        );
    }
}

#[test]
fn adding_notifies_changes_the_scroll_content_id_and_no_glyph_id() {
    assert_ne!(content_id(&fixed_scroll()), content_id(&notifying_scroll()));
    let plain: Vec<ContentId> = fixed_scroll()
        .all_glyphs()
        .into_iter()
        .map(content_id_of_glyph)
        .collect();
    let notifying: Vec<ContentId> = notifying_scroll()
        .all_glyphs()
        .into_iter()
        .map(content_id_of_glyph)
        .collect();
    assert_eq!(plain, notifying);
}

const V4_MANIFEST_BYTES: &[u8] = &[
    4, 5, 48, 46, 49, 46, 48, 1, 51, 122, 136, 245, 203, 142, 118, 114, 246, 150, 92, 105, 64, 96,
    16, 51, 0, 14, 77, 112, 199, 244, 3, 25, 121, 9, 95, 61, 61, 48, 161, 137, 3, 119, 101, 98, 0,
    0, 0, 4, 0, 5, 110, 103, 105, 110, 120, 1, 13, 110, 103, 105, 110, 120, 46, 115, 101, 114, 118,
    105, 99, 101, 2, 21, 47, 101, 116, 99, 47, 110, 103, 105, 110, 120, 47, 110, 103, 105, 110,
    120, 46, 99, 111, 110, 102, 0, 22, 119, 111, 114, 107, 101, 114, 95, 112, 114, 111, 99, 101,
    115, 115, 101, 115, 32, 97, 117, 116, 111, 59, 164, 3, 0, 0, 3, 10, 47, 101, 116, 99, 47, 104,
    111, 115, 116, 115, 19, 49, 50, 55, 46, 48, 46, 48, 46, 49, 32, 108, 111, 99, 97, 108, 104,
    111, 115, 116,
];

#[test]
fn a_real_v3_manifest_is_rejected_as_an_unsupported_format_version() {
    match from_bytes(V3_MANIFEST_BYTES) {
        Err(FromBytesError::UnsupportedFormatVersion { found, supported }) => {
            assert_eq!(found, 3);
            assert_eq!(supported, FORMAT_VERSION);
        }
        other => panic!("expected UnsupportedFormatVersion for v3 bytes, got {other:?}"),
    }
}

#[test]
fn a_real_v4_manifest_is_rejected_as_an_unsupported_format_version() {
    match from_bytes(V4_MANIFEST_BYTES) {
        Err(FromBytesError::UnsupportedFormatVersion { found, supported }) => {
            assert_eq!(found, 4);
            assert_eq!(supported, FORMAT_VERSION);
        }
        other => panic!("expected UnsupportedFormatVersion for v4 bytes, got {other:?}"),
    }
}

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
        .glyphs()
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
fn garbage_bytes_are_undecodable_rather_than_a_bogus_format_version() {
    for garbage in [b"not a manifest".as_slice(), b"".as_slice(), b"{\"a\":1}"] {
        match from_bytes(garbage) {
            Err(FromBytesError::Decode(_)) => {}
            other => panic!(
                "expected a decode failure for {garbage:?}, got {other:?} — a wrong file piped \
                 to golemd must not read as a manifest version"
            ),
        }
    }
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

fn nested_host() -> Scroll {
    Scroll {
        name: "worker-01".to_string(),
        policy: None,
        notifies: vec![],
        contents: Contents::Groups(vec![
            Scroll {
                name: "fishnet".to_string(),
                policy: None,
                notifies: vec![],
                contents: Contents::Groups(vec![Scroll {
                    name: "client-1".to_string(),
                    policy: None,
                    notifies: vec![],
                    contents: Contents::Glyphs(vec![Glyph::AptPackage {
                        name: "stockfish".to_string(),
                    }]),
                }]),
            },
            Scroll {
                name: "base".to_string(),
                policy: None,
                notifies: vec![],
                contents: Contents::Glyphs(vec![Glyph::AptPackage {
                    name: "htop".to_string(),
                }]),
            },
        ]),
    }
}

#[test]
fn nested_scroll_round_trips_through_a_manifest() {
    let manifest = Manifest::from_scrolls(vec![nested_host()], "0.1.0");
    let decoded = from_bytes(&to_bytes(&manifest)).unwrap();
    assert_eq!(decoded, manifest);
    assert_eq!(decoded.format_version, FORMAT_VERSION);
}

// A glyph's content id is over the glyph's own bytes alone, so it does not
// depend on the scroll's name, depth, or grouping — the property that lets
// regrouping re-enact nothing (ADR 0031 §4).
#[test]
fn a_leaf_glyph_content_id_is_independent_of_grouping() {
    let flat = Scroll {
        name: "h".to_string(),
        policy: None,
        notifies: vec![],
        contents: Contents::Glyphs(vec![Glyph::AptPackage {
            name: "stockfish".to_string(),
        }]),
    };
    let grouped = nested_host();
    let g_flat = content_id_of_glyph(flat.all_glyphs()[0]);
    let g_grouped = content_id_of_glyph(
        grouped
            .all_glyphs()
            .into_iter()
            .find(|g| matches!(g, Glyph::AptPackage { name } if name == "stockfish"))
            .unwrap(),
    );
    assert_eq!(g_flat, g_grouped);
}
