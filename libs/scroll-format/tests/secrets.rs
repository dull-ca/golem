use scroll_format::{
    content_id_of_glyph, from_bytes, to_bytes, Chunk, Contents, Entry, FromBytesError, Glyph,
    Manifest, Perms, Scroll, Secret, Text, FORMAT_VERSION,
};

fn sealed(ciphertext: &[u8]) -> Text {
    Text::composed(vec![Chunk::Hole(Secret::Sealed {
        key_id: "fleet-2026".to_string(),
        ciphertext: ciphertext.to_vec(),
    })])
}

fn perms() -> Perms {
    Perms {
        mode: 0o600,
        owner: None,
        group: None,
    }
}

fn file_with(contents: Text) -> Glyph {
    Glyph::Filesystem {
        path: "/etc/app/app.env".to_string(),
        entry: Entry::File {
            contents,
            perms: perms(),
        },
    }
}

#[test]
fn a_sealed_secret_round_trips_through_postcard() {
    let secret = Secret::Sealed {
        key_id: "fleet-2026".to_string(),
        ciphertext: vec![0xde, 0xad, 0xbe, 0xef],
    };
    let bytes = postcard::to_stdvec(&secret).unwrap();
    assert_eq!(postcard::from_bytes::<Secret>(&bytes).unwrap(), secret);
}

#[test]
fn a_reference_secret_round_trips_through_postcard() {
    let secret = Secret::Reference {
        provider: "keyring".to_string(),
        key: "limesurvey_db_password".to_string(),
    };
    let bytes = postcard::to_stdvec(&secret).unwrap();
    assert_eq!(postcard::from_bytes::<Secret>(&bytes).unwrap(), secret);
}

#[test]
fn a_secret_bearing_glyph_round_trips_through_a_manifest() {
    let manifest = Manifest::from_scrolls(
        vec![Scroll {
            name: "app".to_string(),
            policy: None,
            notifies: vec![],
            contents: Contents::Glyphs(vec![
                file_with(sealed(&[1, 2, 3])),
                Glyph::LineInFile {
                    path: "/etc/app/app.env".to_string(),
                    line: sealed(&[4, 5, 6]),
                },
            ]),
        }],
        "0.1.0",
    );
    assert_eq!(from_bytes(&to_bytes(&manifest)).unwrap(), manifest);
}

#[test]
fn identical_ciphertext_gives_identical_content_ids() {
    assert_eq!(
        content_id_of_glyph(&file_with(sealed(&[7, 7, 7, 7]))),
        content_id_of_glyph(&file_with(sealed(&[7, 7, 7, 7]))),
    );
}

#[test]
fn a_rotated_ciphertext_gives_a_different_content_id() {
    assert_ne!(
        content_id_of_glyph(&file_with(sealed(&[7, 7, 7, 7]))),
        content_id_of_glyph(&file_with(sealed(&[8, 8, 8, 8]))),
    );
}

#[test]
fn a_sealed_value_differs_from_the_plain_value_it_seals() {
    assert_ne!(
        content_id_of_glyph(&file_with(Text::Plain("hunter2".to_string()))),
        content_id_of_glyph(&file_with(sealed(b"hunter2"))),
    );
}

#[test]
fn a_key_id_change_alone_moves_the_content_id() {
    let rekeyed = Text::composed(vec![Chunk::Hole(Secret::Sealed {
        key_id: "fleet-2027".to_string(),
        ciphertext: vec![7, 7, 7, 7],
    })]);
    assert_ne!(
        content_id_of_glyph(&file_with(sealed(&[7, 7, 7, 7]))),
        content_id_of_glyph(&file_with(rekeyed)),
    );
}

#[derive(serde::Serialize)]
#[allow(dead_code)]
enum GlyphAtV4 {
    AptPackage { name: String },
    SystemdService { unit: String },
    Filesystem { path: String, entry: EntryAtV4 },
    LineInFile { path: String, line: String },
}

#[derive(serde::Serialize)]
#[allow(dead_code)]
enum EntryAtV4 {
    File { contents: String, perms: Perms },
    Directory { perms: Perms },
    Symlink { target: String },
}

#[test]
fn wrapping_a_value_in_plain_prepends_a_variant_tag_to_the_v4_encoding() {
    let at_v4 = postcard::to_stdvec(&GlyphAtV4::LineInFile {
        path: "/etc/hosts".to_string(),
        line: "127.0.0.1 localhost".to_string(),
    })
    .unwrap();
    let at_v5 = postcard::to_stdvec(&Glyph::LineInFile {
        path: "/etc/hosts".to_string(),
        line: Text::Plain("127.0.0.1 localhost".to_string()),
    })
    .unwrap();

    let tag_offset = at_v4.len() - "\x13127.0.0.1 localhost".len();
    let (prefix, v4_line) = at_v4.split_at(tag_offset);
    let mut tagged = prefix.to_vec();
    tagged.push(0);
    tagged.extend_from_slice(v4_line);

    assert_eq!(at_v5, tagged);
    assert_ne!(at_v5, at_v4);
}

#[test]
fn a_plain_value_does_not_keep_its_v4_content_id() {
    let at_v4 = blake3::hash(
        &postcard::to_stdvec(&GlyphAtV4::LineInFile {
            path: "/etc/hosts".to_string(),
            line: "127.0.0.1 localhost".to_string(),
        })
        .unwrap(),
    );
    let at_v5 = content_id_of_glyph(&Glyph::LineInFile {
        path: "/etc/hosts".to_string(),
        line: Text::Plain("127.0.0.1 localhost".to_string()),
    });
    assert_ne!(at_v5.as_bytes(), at_v4.as_bytes());
}

#[test]
fn a_valueless_glyph_keeps_its_v4_content_id() {
    let at_v4 = blake3::hash(
        &postcard::to_stdvec(&GlyphAtV4::AptPackage {
            name: "nginx".to_string(),
        })
        .unwrap(),
    );
    let at_v5 = content_id_of_glyph(&Glyph::AptPackage {
        name: "nginx".to_string(),
    });
    assert_eq!(at_v5.as_bytes(), at_v4.as_bytes());
}

#[test]
fn a_v5_manifest_decodes_and_reports_the_current_format_version() {
    let manifest = Manifest::from_scrolls(
        vec![Scroll {
            name: "app".to_string(),
            policy: None,
            notifies: vec![],
            contents: Contents::Glyphs(vec![file_with(sealed(&[9, 9]))]),
        }],
        "0.1.0",
    );
    let decoded = from_bytes(&to_bytes(&manifest)).unwrap();
    assert_eq!(decoded.format_version, 5);
    assert_eq!(FORMAT_VERSION, 5);
    match from_bytes(&{
        let mut stale = manifest.clone();
        stale.format_version = 4;
        to_bytes(&stale)
    }) {
        Err(FromBytesError::UnsupportedFormatVersion { found, supported }) => {
            assert_eq!(found, 4);
            assert_eq!(supported, 5);
        }
        other => panic!("expected a v4 manifest to be refused, got {other:?}"),
    }
}

#[test]
fn a_composed_value_keeps_its_literal_chunks_readable_and_only_seals_the_hole() {
    let line = Text::composed(vec![
        Chunk::Lit("Environment=PW=".to_string()),
        Chunk::Hole(Secret::Sealed {
            key_id: "fleet-2026".to_string(),
            ciphertext: vec![1, 2, 3],
        }),
    ]);
    let glyph = Glyph::LineInFile {
        path: "/etc/app.env".to_string(),
        line: line.clone(),
    };
    assert_eq!(
        from_bytes(&to_bytes(&manifest_of(glyph))).unwrap().scrolls[0]
            .scroll
            .glyphs()[0],
        Glyph::LineInFile {
            path: "/etc/app.env".to_string(),
            line
        }
    );
}

#[test]
fn a_composed_value_renders_its_literals_and_redacts_its_holes() {
    let rendered = Text::composed(vec![
        Chunk::Lit("Environment=PW=".to_string()),
        Chunk::Hole(Secret::Sealed {
            key_id: "fleet-2026".to_string(),
            ciphertext: vec![1, 2, 3],
        }),
    ])
    .to_string();
    assert_eq!(
        rendered,
        "Environment=PW=<sealed secret under key fleet-2026>"
    );
}

#[test]
fn composing_only_literals_canonicalizes_to_plain() {
    assert_eq!(
        Text::composed(vec![
            Chunk::Lit("Environment=".to_string()),
            Chunk::Lit("PW=x".to_string()),
        ]),
        Text::Plain("Environment=PW=x".to_string())
    );
}

#[test]
fn composing_merges_adjacent_literals_so_one_meaning_has_one_encoding() {
    let split = Text::composed(vec![
        Chunk::Lit("Environment=".to_string()),
        Chunk::Lit("PW=".to_string()),
        Chunk::Hole(Secret::Sealed {
            key_id: "k".to_string(),
            ciphertext: vec![1],
        }),
        Chunk::Lit(String::new()),
    ]);
    let merged = Text::composed(vec![
        Chunk::Lit("Environment=PW=".to_string()),
        Chunk::Hole(Secret::Sealed {
            key_id: "k".to_string(),
            ciphertext: vec![1],
        }),
    ]);
    assert_eq!(split, merged);
    assert_eq!(
        content_id_of_glyph(&file_with(split)),
        content_id_of_glyph(&file_with(merged))
    );
}

#[test]
fn a_composed_key_fragment_discriminates_two_different_sealed_lines() {
    let hole = |ciphertext: Vec<u8>| {
        Text::composed(vec![
            Chunk::Lit("PW=".to_string()),
            Chunk::Hole(Secret::Sealed {
                key_id: "k".to_string(),
                ciphertext,
            }),
        ])
    };
    assert_ne!(hole(vec![1]).key_fragment(), hole(vec![2]).key_fragment());
    assert!(hole(vec![1]).key_fragment().starts_with("PW="));
}

fn manifest_of(glyph: Glyph) -> Manifest {
    Manifest::from_scrolls(
        vec![Scroll {
            name: "app".to_string(),
            policy: None,
            notifies: vec![],
            contents: Contents::Glyphs(vec![glyph]),
        }],
        "0.1.0",
    )
}
