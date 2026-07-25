use scroll_format::{
    content_id, content_id_of_glyph, from_bytes, to_bytes, Contents, ContentId, Entry,
    FromBytesError, Glyph, Manifest, Perms, Scroll, FORMAT_VERSION,
};

fn fixed_scroll() -> Scroll {
    Scroll {
        name: "web".to_string(),
        policy: None,
        contents: Contents::Glyphs(vec![
            Glyph::AptPackage { name: "nginx".to_string() },
            Glyph::SystemdService { unit: "nginx.service".to_string() },
            Glyph::Filesystem {
                path: "/etc/nginx/nginx.conf".to_string(),
                entry: Entry::File {
                    contents: "worker_processes auto;".to_string(),
                    perms: Perms { mode: 0o644, owner: None, group: None },
                },
            },
            Glyph::LineInFile {
                path: "/etc/hosts".to_string(),
                line: "127.0.0.1 localhost".to_string(),
            },
        ]),
    }
}

fn other_scroll() -> Scroll {
    Scroll {
        name: "db".to_string(),
        policy: None,
        contents: Contents::Glyphs(vec![Glyph::AptPackage {
            name: "postgresql".to_string(),
        }]),
    }
}

const GOLDEN_SCROLL_BYTES: &[u8] = &[
    3, 119, 101, 98, 0, 0, 4, 0, 5, 110, 103, 105, 110, 120, 1, 13, 110, 103, 105, 110, 120, 46,
    115, 101, 114, 118, 105, 99, 101, 2, 21, 47, 101, 116, 99, 47, 110, 103, 105, 110, 120, 47,
    110, 103, 105, 110, 120, 46, 99, 111, 110, 102, 0, 22, 119, 111, 114, 107, 101, 114, 95, 112,
    114, 111, 99, 101, 115, 115, 101, 115, 32, 97, 117, 116, 111, 59, 164, 3, 0, 0, 3, 10, 47,
    101, 116, 99, 47, 104, 111, 115, 116, 115, 19, 49, 50, 55, 46, 48, 46, 48, 46, 49, 32, 108,
    111, 99, 97, 108, 104, 111, 115, 116,
];

const GOLDEN_CONTENT_ID: &str =
    "b98f514234d172aad5972bad2f9de3aad9c0be9612702d78885725ddb28661f0";

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
        contents: Contents::Groups(vec![
            Scroll {
                name: "fishnet".to_string(),
                policy: None,
                contents: Contents::Groups(vec![Scroll {
                    name: "client-1".to_string(),
                    policy: None,
                    contents: Contents::Glyphs(vec![Glyph::AptPackage {
                        name: "stockfish".to_string(),
                    }]),
                }]),
            },
            Scroll {
                name: "base".to_string(),
                policy: None,
                contents: Contents::Glyphs(vec![Glyph::AptPackage { name: "htop".to_string() }]),
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

#[test]
fn a_leaf_glyph_content_id_is_independent_of_grouping() {
    let flat = Scroll {
        name: "h".to_string(),
        policy: None,
        contents: Contents::Glyphs(vec![Glyph::AptPackage { name: "stockfish".to_string() }]),
    };
    let grouped = nested_host();
    let g_flat = content_id_of_glyph(flat.all_glyphs()[0]);
    let g_grouped = content_id_of_glyph(
        grouped.all_glyphs().into_iter().find(|g| matches!(g, Glyph::AptPackage { name } if name == "stockfish")).unwrap(),
    );
    assert_eq!(g_flat, g_grouped);
}
