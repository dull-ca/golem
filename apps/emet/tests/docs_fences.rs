mod docs_gate;

use std::path::{Path, PathBuf};

use docs_gate::{
    fail_with, line_of_byte, markdown_pages_under, phase_label, relative_to_repo, repo_root,
};
use emet::{compile_file_all, Error};

const FRAGMENT_MARKER: &str = "fragment";
// Two spellings of one language. The site ships its own Emet grammar
// (`sites/website/src/grammars/emet.tmLanguage.json`) and its pages say `emet`;
// `docs/guide/` is plain markdown read through GitHub, which has an `elm`
// highlighter and no `emet`, so those pages borrow Elm's. A fence is governed
// by what it contains, not by which of the two words it was tagged with.
const EMET_FENCE_LANGUAGES: [&str; 2] = ["emet", "elm"];
const FENCE_DELIMITER: &str = "```";
// The trees that teach. `docs/adr` and `docs/design` are deliberately absent:
// they record what was decided when it was decided, so a fence in them is a
// dated quotation, and editing one until it compiles would falsify the record.
const INSTRUCTIONAL_PAGE_ROOTS: [&str; 2] = ["docs/guide", "sites/website/src/content/docs"];
const LIBRARY_MANIFEST: &str = "emet.json";
const LIBRARY_DIRECTORY: &str = "lib";
const FENCE_ENTRY_FILENAME: &str = "fence.emet";
const SYNTHETIC_MAIN: &str = "\nmain : List Scroll\nmain = []\n";

struct EmetFence {
    page: PathBuf,
    opening_line: usize,
    marked_fragment: bool,
    body: String,
}

impl EmetFence {
    fn location(&self) -> String {
        format!("{}:{}", relative_to_repo(&self.page), self.opening_line)
    }

    // A page teaching a signature or a helper should not have to carry a `main`
    // to be checked, so one is supplied. The two ways the guess below can be
    // wrong both surface as a compile error the author sees — a missing `main`,
    // or a duplicate declaration — never as a fence that silently escapes.
    fn standalone_program(&self) -> String {
        if self.declares_main() {
            self.body.clone()
        } else {
            format!("{}\n{SYNTHETIC_MAIN}", self.body)
        }
    }

    fn declares_main(&self) -> bool {
        self.body.lines().any(|line| {
            line.strip_prefix("main")
                .is_some_and(|rest| !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_'))
        })
    }
}

struct FenceWorkspace {
    dir: tempfile::TempDir,
}

impl FenceWorkspace {
    // The search path is the repository's real `lib/`, never a stub. A page
    // that writes a `Quadlet.Workload` literal is then checked against the
    // library golem actually ships, so adding or dropping a field on that
    // record fails the page instead of leaving the page describing a library
    // that no longer exists — nine such literals sat three fields out of date
    // before this gate (ADR 0054, which widens ADR 0043 to cover fences).
    fn resolving_shipped_libraries() -> Self {
        let dir = tempfile::tempdir().expect("a writable temporary directory for fence programs");
        let libraries = repo_root().join(LIBRARY_DIRECTORY);
        std::fs::write(
            dir.path().join(LIBRARY_MANIFEST),
            format!(
                "{{ \"source-directories\": [{}] }}",
                serde_json::to_string(&libraries.display().to_string())
                    .expect("a path is representable as JSON")
            ),
        )
        .expect("the fence workspace manifest is writable");
        Self { dir }
    }

    fn compile(&self, fence: &EmetFence) -> Result<(), Vec<Error>> {
        let entry = self.dir.path().join(FENCE_ENTRY_FILENAME);
        std::fs::write(&entry, fence.standalone_program()).expect("the fence entry is writable");
        compile_file_all(&entry).map(|_| ())
    }
}

fn emet_fences_in(page: &Path) -> Vec<EmetFence> {
    let text = std::fs::read_to_string(page)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", page.display()));
    let lines: Vec<&str> = text.lines().collect();

    let mut fences = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let Some(info) = lines[index].trim_start().strip_prefix(FENCE_DELIMITER) else {
            index += 1;
            continue;
        };

        let mut words = info.split_whitespace();
        let language = words.next().unwrap_or_default();
        let marked_fragment = words.any(|word| word == FRAGMENT_MARKER);

        let mut close = index + 1;
        while close < lines.len() && !lines[close].trim_start().starts_with(FENCE_DELIMITER) {
            close += 1;
        }

        if EMET_FENCE_LANGUAGES.contains(&language) {
            fences.push(EmetFence {
                page: page.to_path_buf(),
                opening_line: index + 1,
                marked_fragment,
                body: lines[index + 1..close].join("\n"),
            });
        }
        index = close + 1;
    }
    fences
}

fn documented_fences() -> Vec<EmetFence> {
    let mut fences = Vec::new();
    for root in INSTRUCTIONAL_PAGE_ROOTS {
        for page in markdown_pages_under(&repo_root().join(root)) {
            fences.extend(emet_fences_in(&page));
        }
    }

    assert!(
        !fences.is_empty(),
        "no ```emet fences found under {INSTRUCTIONAL_PAGE_ROOTS:?} — the instructional pages are \
         empty or unreachable"
    );
    fences
}

fn describe(fence: &EmetFence, errors: &[Error]) -> String {
    let program = fence.standalone_program();
    let entry = FENCE_ENTRY_FILENAME;

    let mut described = String::new();
    for e in errors {
        let inside_the_fence = e
            .file
            .as_ref()
            .and_then(|f| f.file_name())
            .is_none_or(|name| name == entry);
        let location = if inside_the_fence {
            format!(
                "{}:{}",
                relative_to_repo(&fence.page),
                fence.opening_line + line_of_byte(&program, e.span.start)
            )
        } else {
            relative_to_repo(
                e.file
                    .as_ref()
                    .expect("an out-of-fence error names its file"),
            )
        };
        described.push_str(&format!(
            "  {location}: {}: {}\n",
            phase_label(e.phase),
            e.msg
        ));
        if let Some(note) = &e.note {
            described.push_str(&format!("    note: {note}\n"));
        }
    }
    described
}

#[test]
fn every_documented_emet_fence_compiles() {
    let workspace = FenceWorkspace::resolving_shipped_libraries();
    let mut failures = Vec::new();

    for fence in documented_fences() {
        if fence.marked_fragment {
            continue;
        }
        if let Err(errors) = workspace.compile(&fence) {
            failures.push(format!(
                "{}: this ```emet block does not compile:\n{}  if it is not a program on its own, \
                 say so in the fence: ```{} {FRAGMENT_MARKER}\n",
                fence.location(),
                describe(&fence, &errors),
                EMET_FENCE_LANGUAGES[0]
            ));
        }
    }

    fail_with("documented Emet fence(s) do not compile", failures);
}

// `fragment` is the one way out of the gate above, so on its own it is also the
// one way to silence the gate: an author facing a diagnostic can type the word
// and the page goes green. Compiling the marked fences too, and failing the
// ones that succeed, makes the marker cost something — it may only be claimed
// by a fence that genuinely is not a program, which is the claim it makes
// (ADR 0054).
#[test]
fn every_fragment_marker_is_earned() {
    let workspace = FenceWorkspace::resolving_shipped_libraries();
    let mut failures = Vec::new();

    for fence in documented_fences() {
        if !fence.marked_fragment {
            continue;
        }
        if workspace.compile(&fence).is_ok() {
            failures.push(format!(
                "{}: marked `{FRAGMENT_MARKER}`, but it compiles as a program on its own — drop \
                 the marker so the checker governs it\n",
                fence.location()
            ));
        }
    }

    fail_with("fence(s) claim to be fragments and are not", failures);
}
