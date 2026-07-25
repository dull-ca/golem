//! The pure diff: desired scroll vs. last-applied outcomes in, an ordered list
//! of glyph operations out, no side effects (ADR 0014 §3, ADR 0015 §2). The
//! foreman enacts what this decides.

use scroll_format::{content_id_of_glyph, ContentId, Glyph, Scroll};

use crate::journal::{GlyphOp, Outcome};

/// The content id `plan` versions a glyph by. Delegates to
/// `scroll_format::content_id_of_glyph` so the hash has one definition shared
/// with the compiler (ADR 0013).
pub fn glyph_content_id(glyph: &Glyph) -> ContentId {
    content_id_of_glyph(glyph)
}

/// Diff the desired scroll against the prior applied outcomes, keyed by
/// [`Glyph::key`] (the stable per-resource identity) and versioned by content
/// id. Per key: absent from prior → `Install`; present with the same id →
/// `Noop`; present with a different id → `Replace` (an upgrade); present in prior
/// but gone from desired → `Remove`. Installs and replaces come first in desired
/// order; removes last, in reverse (ADR 0029 §6), so the foreman applies
/// additions before undoing what left.
pub fn plan(prior: &[Outcome], desired: &Scroll) -> Vec<GlyphOp> {
    let mut ops = Vec::new();
    let mut seen = std::collections::BTreeSet::new();

    // NOTE: the diff is per-glyph, not per-group — group structure is not an
    // input (ADR 0031 §4). Flattening every leaf's glyphs in source order yields
    // the same ops however they are grouped, so regrouping or renaming a group
    // re-enacts nothing: no glyph's key or content id depends on its enclosing
    // scroll's name or depth.
    for glyph in desired.all_glyphs() {
        let key = glyph.key();
        seen.insert(key.clone());
        let new_cid = glyph_content_id(glyph);
        match prior.iter().find(|o| o.op.key() == key) {
            None => ops.push(GlyphOp::Install { cid: new_cid, glyph: glyph.clone() }),
            Some(prev) if prev.cid == new_cid => {
                ops.push(GlyphOp::Noop { cid: new_cid, glyph: glyph.clone() })
            }
            Some(prev) => ops.push(GlyphOp::Replace {
                old_cid: prev.cid,
                new_cid,
                glyph: glyph.clone(),
            }),
        }
    }

    // NOTE: removes unwind in reverse of apply order (reverse prior order), so a
    // dependent glyph tears down before the one it depended on (ADR 0029 §6).
    // Install/replace order above is unchanged.
    for prev in prior.iter().rev() {
        if !seen.contains(&prev.op.key()) {
            ops.push(GlyphOp::Remove {
                cid: prev.cid,
                glyph: prev.op.glyph().clone(),
            });
        }
    }

    ops
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::Inverse;

    fn apt(name: &str) -> Glyph {
        Glyph::AptPackage { name: name.into() }
    }

    fn file(path: &str, contents: &str) -> Glyph {
        Glyph::Filesystem {
            path: path.into(),
            entry: scroll_format::Entry::File {
                contents: contents.into(),
                perms: scroll_format::Perms { mode: 0o644, owner: None, group: None },
            },
        }
    }

    fn scroll(glyphs: Vec<Glyph>) -> Scroll {
        Scroll { name: "h1".into(), policy: None, contents: scroll_format::Contents::Glyphs(glyphs) }
    }

    fn nested(children: Vec<(&str, Vec<Glyph>)>) -> Scroll {
        Scroll {
            name: "host".into(),
            policy: None,
            contents: scroll_format::Contents::Groups(
                children
                    .into_iter()
                    .map(|(name, glyphs)| Scroll {
                        name: name.into(),
                        policy: None,
                        contents: scroll_format::Contents::Glyphs(glyphs),
                    })
                    .collect(),
            ),
        }
    }

    fn applied(glyph: Glyph) -> Outcome {
        Outcome {
            cid: glyph_content_id(&glyph),
            op: GlyphOp::Install { cid: glyph_content_id(&glyph), glyph: glyph.clone() },
            inverse: Inverse::Nothing,
            changed: true,
        }
    }

    #[test]
    fn new_glyph_against_empty_prior_is_install() {
        let ops = plan(&[], &scroll(vec![apt("nginx")]));
        assert_eq!(ops, vec![GlyphOp::Install { cid: glyph_content_id(&apt("nginx")), glyph: apt("nginx") }]);
    }

    #[test]
    fn unchanged_glyph_is_noop() {
        let ops = plan(&[applied(apt("nginx"))], &scroll(vec![apt("nginx")]));
        assert_eq!(ops, vec![GlyphOp::Noop { cid: glyph_content_id(&apt("nginx")), glyph: apt("nginx") }]);
    }

    #[test]
    fn same_key_changed_contents_is_replace() {
        let prior = vec![applied(file("/etc/app.conf", "old"))];
        let ops = plan(&prior, &scroll(vec![file("/etc/app.conf", "new")]));
        assert_eq!(
            ops,
            vec![GlyphOp::Replace {
                old_cid: glyph_content_id(&file("/etc/app.conf", "old")),
                new_cid: glyph_content_id(&file("/etc/app.conf", "new")),
                glyph: file("/etc/app.conf", "new"),
            }]
        );
    }

    #[test]
    fn glyph_only_in_prior_is_remove() {
        let ops = plan(&[applied(apt("nginx"))], &scroll(vec![]));
        assert_eq!(ops, vec![GlyphOp::Remove { cid: glyph_content_id(&apt("nginx")), glyph: apt("nginx") }]);
    }

    #[test]
    fn plan_flattens_nested_leaves_in_source_order() {
        let desired = nested(vec![("a", vec![apt("one")]), ("b", vec![apt("two")])]);
        let ops = plan(&[], &desired);
        assert_eq!(
            ops,
            vec![
                GlyphOp::Install { cid: glyph_content_id(&apt("one")), glyph: apt("one") },
                GlyphOp::Install { cid: glyph_content_id(&apt("two")), glyph: apt("two") },
            ]
        );
    }

    #[test]
    fn removes_come_out_in_reverse_prior_order() {
        let prior = vec![applied(apt("first")), applied(apt("second")), applied(apt("third"))];
        let ops = plan(&prior, &scroll(vec![]));
        assert_eq!(
            ops,
            vec![
                GlyphOp::Remove { cid: glyph_content_id(&apt("third")), glyph: apt("third") },
                GlyphOp::Remove { cid: glyph_content_id(&apt("second")), glyph: apt("second") },
                GlyphOp::Remove { cid: glyph_content_id(&apt("first")), glyph: apt("first") },
            ]
        );
    }

    #[test]
    fn installs_precede_removes_and_follow_desired_order() {
        let prior = vec![applied(apt("old"))];
        let ops = plan(&prior, &scroll(vec![apt("a"), apt("b")]));
        assert_eq!(
            ops,
            vec![
                GlyphOp::Install { cid: glyph_content_id(&apt("a")), glyph: apt("a") },
                GlyphOp::Install { cid: glyph_content_id(&apt("b")), glyph: apt("b") },
                GlyphOp::Remove { cid: glyph_content_id(&apt("old")), glyph: apt("old") },
            ]
        );
    }
}
