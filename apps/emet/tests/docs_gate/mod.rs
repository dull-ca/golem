// What `docs_examples`, `docs_fences`, and `docs_links` share: where the
// repository is, which files count as prose, and how a failure reads.
//
// NOTE: `mod docs_gate;` compiles a separate private copy into each test binary
// that declares it, and no one binary uses the whole surface — without the
// allow, each would warn about the helpers the other two need.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use emet::{Error, Phase};

pub const MARKDOWN_EXTENSIONS: [&str; 2] = ["md", "mdx"];

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root is two levels above the emet crate")
}

pub fn relative_to_repo(path: &Path) -> String {
    path.strip_prefix(repo_root())
        .unwrap_or(path)
        .display()
        .to_string()
}

pub fn markdown_pages_under(dir: &Path) -> Vec<PathBuf> {
    if dir.is_file() {
        return vec![dir.to_path_buf()];
    }

    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| {
        panic!(
            "cannot read the documentation tree at {}: {e} — if this fires under `nix build`, \
             the flake's source filter is excluding it from the workspace test sandbox",
            dir.display()
        )
    });

    let mut found = Vec::new();
    for entry in entries {
        let path = entry.expect("readable directory entry").path();
        if path.is_dir() {
            found.extend(markdown_pages_under(&path));
            continue;
        }
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if MARKDOWN_EXTENSIONS.contains(&extension) {
            found.push(path);
        }
    }
    found.sort();
    found
}

pub fn line_of_byte(source: &str, byte: usize) -> usize {
    let mut boundary = byte.min(source.len());
    while !source.is_char_boundary(boundary) {
        boundary -= 1;
    }
    source[..boundary].matches('\n').count() + 1
}

pub fn phase_label(phase: Phase) -> &'static str {
    match phase {
        Phase::Lex => "lex error",
        Phase::Parse => "parse error",
        Phase::Type => "type error",
        Phase::Analyze => "analysis error",
    }
}

pub fn render_diagnostics(errors: &[Error]) -> String {
    let mut rendered = String::new();
    for e in errors {
        rendered.push_str(&format!("{}: {}\n", phase_label(e.phase), e.msg));
        if let Some(note) = &e.note {
            rendered.push_str(&format!("  note: {note}\n"));
        }
    }
    rendered
}

// NOTE: the subject carries its own ADR. The three suites answer to two
// different decisions — ADR 0043 for the examples tree and its goldens, ADR
// 0054 for the fences, the links, and the mirror sidecars — so a citation
// fixed here would misattribute two of them.
pub fn fail_with(subject: &str, failures: Vec<String>) {
    if failures.is_empty() {
        return;
    }
    panic!("{} {subject}:\n\n{}", failures.len(), failures.join("\n"));
}
