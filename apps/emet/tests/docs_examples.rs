mod docs_gate;

use std::path::{Path, PathBuf};

use docs_gate::{fail_with, relative_to_repo, render_diagnostics, repo_root};
use emet::{compile_file_all, render_text};

const GOLDEN_UPDATE_ENV: &str = "UPDATE_DOCS_GOLDEN";
const PROGRAM_EXTENSION: &str = "emet";
const REFERENCE_EXTENSION: &str = "emet-ref";
const EXPECTED_ERROR_SUFFIX: &str = ".expected-error";
const GOLDEN_SUFFIX: &str = ".text.golden";
const MIRRORS_SUFFIX: &str = ".mirrors";

struct DocsExample {
    declared_at: PathBuf,
    program: PathBuf,
    expected_error: Option<PathBuf>,
    golden: Option<PathBuf>,
    mirrors: Option<PathBuf>,
}

fn docs_examples_dir() -> PathBuf {
    repo_root().join("sites").join("website").join("examples")
}

fn emet_files_under(dir: &Path) -> Vec<PathBuf> {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| {
        panic!(
            "cannot read the docs example tree at {}: {e} — if this fires under `nix build`, \
             the flake's source filter is excluding sites/website/examples (ADR 0043)",
            dir.display()
        )
    });

    let mut found = Vec::new();
    for entry in entries {
        let path = entry.expect("readable directory entry").path();
        if path.is_dir() {
            found.extend(emet_files_under(&path));
            continue;
        }
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if extension == PROGRAM_EXTENSION || extension == REFERENCE_EXTENSION {
            found.push(path);
        }
    }
    found.sort();
    found
}

fn sidecar(declared_at: &Path, suffix: &str) -> Option<PathBuf> {
    let stem = declared_at
        .file_stem()
        .and_then(|s| s.to_str())
        .expect("example file has a stem");
    let path = declared_at.with_file_name(format!("{stem}{suffix}"));
    path.exists().then_some(path)
}

fn repo_path_named_in(sidecar_at: &Path) -> PathBuf {
    let target = std::fs::read_to_string(sidecar_at)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", sidecar_at.display()));
    repo_root().join(target.trim())
}

fn docs_examples() -> Vec<DocsExample> {
    let dir = docs_examples_dir();
    let declarations = emet_files_under(&dir);

    assert!(
        !declarations.is_empty(),
        "no examples found under {} — the docs example tree is empty or unreachable",
        dir.display()
    );

    declarations
        .into_iter()
        .map(|declared_at| {
            let program =
                if declared_at.extension().and_then(|e| e.to_str()) == Some(REFERENCE_EXTENSION) {
                    repo_path_named_in(&declared_at)
                } else {
                    declared_at.clone()
                };
            DocsExample {
                expected_error: sidecar(&declared_at, EXPECTED_ERROR_SUFFIX),
                golden: sidecar(&declared_at, GOLDEN_SUFFIX),
                mirrors: sidecar(&declared_at, MIRRORS_SUFFIX),
                declared_at,
                program,
            }
        })
        .collect()
}

fn unified_diff(expected: &str, actual: &str) -> String {
    let expected_lines: Vec<&str> = expected.lines().collect();
    let actual_lines: Vec<&str> = actual.lines().collect();
    let mut diff = String::new();
    for line in 0..expected_lines.len().max(actual_lines.len()) {
        let want = expected_lines.get(line).copied();
        let got = actual_lines.get(line).copied();
        if want == got {
            continue;
        }
        diff.push_str(&format!("  line {}:\n", line + 1));
        diff.push_str(&format!("    expected: {}\n", want.unwrap_or("<missing>")));
        diff.push_str(&format!("    actual: {}\n", got.unwrap_or("<missing>")));
    }
    diff
}

#[test]
fn every_docs_example_compiles_or_fails_as_recorded() {
    let mut failures = Vec::new();

    for example in docs_examples() {
        let name = relative_to_repo(&example.declared_at);
        let outcome = compile_file_all(&example.program);

        match (&example.expected_error, outcome) {
            (None, Ok(_)) => {}
            (None, Err(errors)) => failures.push(format!(
                "{name}: expected it to compile, but the compiler said:\n{}",
                render_diagnostics(&errors)
            )),
            (Some(expected_at), Ok(_)) => failures.push(format!(
                "{name}: recorded as a failing example in {}, but it compiled cleanly — \
                 the language changed, so either the lesson or the recorded error is stale\n",
                relative_to_repo(expected_at)
            )),
            (Some(expected_at), Err(errors)) => {
                let expected = std::fs::read_to_string(expected_at)
                    .unwrap_or_else(|e| panic!("cannot read {}: {e}", expected_at.display()));
                let actual = render_diagnostics(&errors);
                if !actual.contains(expected.trim_end()) {
                    failures.push(format!(
                        "{name}: failed, but not with the diagnostic recorded in {}\n  \
                         expected to contain: {}\n  actual:\n{}",
                        relative_to_repo(expected_at),
                        expected.trim_end(),
                        actual
                    ));
                }
            }
        }
    }

    fail_with("docs example(s) broke", failures);
}

#[test]
fn every_docs_golden_matches_rendered_output() {
    let updating = std::env::var_os(GOLDEN_UPDATE_ENV).is_some();
    let mut failures = Vec::new();

    for example in docs_examples() {
        let Some(golden_at) = example.golden else {
            continue;
        };
        let name = relative_to_repo(&example.declared_at);

        let compiled = match compile_file_all(&example.program) {
            Ok(compiled) => compiled,
            Err(errors) => {
                failures.push(format!(
                    "{name}: has a golden but no longer compiles:\n{}",
                    render_diagnostics(&errors)
                ));
                continue;
            }
        };
        let actual = render_text(&compiled);

        if updating {
            std::fs::write(&golden_at, &actual)
                .unwrap_or_else(|e| panic!("cannot write {}: {e}", golden_at.display()));
            continue;
        }

        let expected = std::fs::read_to_string(&golden_at)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", golden_at.display()));
        if expected != actual {
            failures.push(format!(
                "{name}: rendered output no longer matches {} \
                 (rerun with {GOLDEN_UPDATE_ENV}=1 if the change is intended)\n{}",
                relative_to_repo(&golden_at),
                unified_diff(&expected, &actual)
            ));
        }
    }

    fail_with("docs golden(s) drifted", failures);
}

// ADR 0043 says a page never carries a copy of a program, and a `.emet-ref` is
// how a page shows the real one verbatim. A mirror is the case that cannot
// serve: the page wants the program rearranged for teaching — declarations
// moved so `#region` can skip the ones the lesson is not about, the module
// header and the design comments gone — while the deployable program stays
// under the repo-root `examples/`. Two files then exist where ADR 0043 wanted
// one, and this is what keeps them one program: whatever the rearranging did
// to the text, both must still plan exactly the same thing.
#[test]
fn every_mirrored_example_plans_what_the_real_program_plans() {
    let mut failures = Vec::new();

    for example in docs_examples() {
        let Some(mirrors_at) = example.mirrors else {
            continue;
        };
        let name = relative_to_repo(&example.declared_at);
        let mirrored = repo_path_named_in(&mirrors_at);

        let rendered = |program: &Path| match compile_file_all(program) {
            Ok(compiled) => Ok(render_text(&compiled)),
            Err(errors) => Err(format!(
                "{}:\n{}",
                relative_to_repo(program),
                render_diagnostics(&errors)
            )),
        };

        match (rendered(&example.program), rendered(&mirrored)) {
            (Err(broken), _) | (_, Err(broken)) => {
                failures.push(format!("{name}: cannot compare, because {broken}"))
            }
            (Ok(shown), Ok(real)) if shown != real => failures.push(format!(
                "{name}: no longer plans what {} plans — the page is showing a rendition of a \
                 real program, and the two have diverged in substance (formatting and comments \
                 are free to differ; the plan is not)\n{}",
                relative_to_repo(&mirrored),
                unified_diff(&real, &shown)
            )),
            (Ok(_), Ok(_)) => {}
        }
    }

    fail_with("mirrored example(s) drifted", failures);
}
