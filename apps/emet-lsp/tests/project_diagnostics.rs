use std::fs;
use std::path::PathBuf;

use emet_lsp::{completion_at, definition_at, diagnostics_for, hover_at};
use lsp_types::{Position, Uri};

const LIBRARY: &str = "module Shapes exposing (Shape(..), describe)\n\ntype Shape = Circle Int | Square Int\n\ndescribe : Shape -> String\ndescribe shape =\n  case shape of\n    Circle _ ->\n      \"round\"\n    Square _ ->\n      \"boxy\"\n";

const ENTRY: &str = "import Shapes exposing (Shape(..), describe)\n\nunitShape : Shape\nunitShape = Circle 1\n\nmain : List Scroll\nmain =\n  let _named = describe unitShape\n  in []\n";

struct Project {
    root: PathBuf,
    entry: PathBuf,
}

impl Project {
    fn uri(&self) -> Uri {
        format!("file://{}", self.entry.display()).parse().unwrap()
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn project_with_library(tag: &str, entry_source: &str) -> Project {
    project(tag, LIBRARY, entry_source)
}

fn project(tag: &str, library_source: &str, entry_source: &str) -> Project {
    let root = std::env::temp_dir().join(format!("emet_lsp_project_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let library_dir = root.join("lib");
    fs::create_dir_all(&library_dir).unwrap();
    fs::write(
        root.join("emet.json"),
        "{ \"source-directories\": [\"lib\"] }",
    )
    .unwrap();
    fs::write(library_dir.join("Shapes.emet"), library_source).unwrap();
    let entry = root.join("Main.emet");
    fs::write(&entry, entry_source).unwrap();
    Project { root, entry }
}

#[test]
fn imported_type_in_an_annotation_is_not_an_unknown_constructor() {
    let project = project_with_library("imported_type", ENTRY);
    let diagnostics = diagnostics_for(&project.uri(), ENTRY);
    assert!(
        diagnostics.is_empty(),
        "expected no diagnostics, got: {:?}",
        diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn unsaved_buffer_wins_over_the_file_on_disk() {
    let project = project_with_library("dirty_buffer", ENTRY);
    let dirty = ENTRY.replace("Circle 1", "Circle \"one\"");
    let diagnostics = diagnostics_for(&project.uri(), &dirty);
    assert_eq!(
        diagnostics.len(),
        1,
        "the buffer's type error, not the clean file on disk: {:?}",
        diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn a_missing_imported_module_is_diagnosed_at_its_import() {
    let entry_source = "import Absent exposing (thing)\n\nmain : List Scroll\nmain =\n  []\n";
    let project = project_with_library("missing_module", entry_source);
    let diagnostics = diagnostics_for(&project.uri(), entry_source);
    assert_eq!(diagnostics.len(), 1);
    assert!(
        diagnostics[0]
            .message
            .contains("cannot find imported module `Absent`"),
        "message: {}",
        diagnostics[0].message
    );
    assert_eq!(diagnostics[0].range.start.line, 0);
}

#[test]
fn a_library_that_fails_to_type_check_leaves_its_importer_analyzable() {
    let broken_library =
        "module Shapes exposing (Shape(..), describe)\n\ntype Shape = Circle Int\n\ndescribe : Shape -> String\ndescribe _shape =\n  undefinedInLibrary\n";
    let project = project("broken_library", broken_library, ENTRY);
    let diagnostics = diagnostics_for(&project.uri(), ENTRY);
    let entry_lines = ENTRY.lines().count() as u32;
    for diagnostic in &diagnostics {
        assert!(
            diagnostic.range.start.line < entry_lines,
            "diagnostic outside the entry file: {diagnostic:?}"
        );
    }
}

#[test]
fn a_pathless_buffer_still_analyzes_single_file() {
    let untitled: Uri = "untitled:Untitled-1".parse().unwrap();
    let source = "main : List Scroll\nmain =\n  undefinedThing\n";
    let diagnostics = diagnostics_for(&untitled, source);
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].message.contains("undefinedThing"));
}

#[test]
fn hover_reports_the_type_of_an_imported_value() {
    let project = project_with_library("hover_import", ENTRY);
    let line = ENTRY.lines().position(|l| l.contains("_named")).unwrap() as u32;
    let column = ENTRY
        .lines()
        .nth(line as usize)
        .unwrap()
        .find("describe")
        .unwrap() as u32;
    let hover = hover_at(&project.uri(), ENTRY, Position::new(line, column))
        .expect("hover over the imported `describe`");
    let text = match hover.contents {
        lsp_types::HoverContents::Markup(markup) => markup.value,
        _ => panic!("expected markup contents"),
    };
    assert!(text.contains("Shape"), "hover text: {text}");
}

#[test]
fn completion_offers_imported_names() {
    let project = project_with_library("completion_import", ENTRY);
    let line = ENTRY.lines().position(|l| l.contains("_named")).unwrap() as u32;
    let items = completion_at(&project.uri(), ENTRY, Position::new(line, 20));
    assert!(
        items.iter().any(|item| item.label == "describe"),
        "completion labels: {:?}",
        items.iter().map(|i| &i.label).collect::<Vec<_>>()
    );
}

#[test]
fn definition_of_an_imported_name_opens_the_library_module() {
    let project = project_with_library("definition_import", ENTRY);
    let line = ENTRY.lines().position(|l| l.contains("_named")).unwrap() as u32;
    let column = ENTRY
        .lines()
        .nth(line as usize)
        .unwrap()
        .find("describe")
        .unwrap() as u32;
    let location = definition_at(&project.uri(), ENTRY, Position::new(line, column))
        .expect("a cross-file definition location");
    assert!(
        location.uri.as_str().ends_with("lib/Shapes.emet"),
        "definition uri: {}",
        location.uri.as_str()
    );
    let definition_line = LIBRARY
        .lines()
        .position(|l| l.starts_with("describe shape"))
        .unwrap() as u32;
    assert_eq!(location.range.start.line, definition_line);
}

const DOCUMENTED_LIBRARY: &str = "module Shapes exposing (Shape(..), describe)\n\ntype Shape = Circle Int | Square Int\n\n-- Names a shape in one word.\n--\n-- Total: every shape has a name.\ndescribe : Shape -> String\ndescribe shape =\n  case shape of\n    Circle _ ->\n      \"round\"\n    Square _ ->\n      \"boxy\"\n";

fn hover_text(uri: &Uri, source: &str, line_needle: &str, word: &str) -> String {
    let line = source
        .lines()
        .position(|l| l.contains(line_needle))
        .unwrap() as u32;
    let column = source
        .lines()
        .nth(line as usize)
        .unwrap()
        .rfind(word)
        .unwrap() as u32;
    let hover = hover_at(uri, source, Position::new(line, column)).expect("a hover");
    match hover.contents {
        lsp_types::HoverContents::Markup(markup) => markup.value,
        _ => panic!("expected markup contents"),
    }
}

#[test]
fn hover_on_a_documented_imported_value_carries_signature_prose_and_origin() {
    let project = project("documented_import", DOCUMENTED_LIBRARY, ENTRY);
    let text = hover_text(&project.uri(), ENTRY, "describe unitShape", "describe");
    assert!(
        text.contains("```emet\nShape -> String\n```"),
        "hover text: {text}"
    );
    assert!(
        text.contains("Names a shape in one word."),
        "hover text: {text}"
    );
    assert!(
        text.contains("Total: every shape has a name."),
        "hover text: {text}"
    );
    assert!(text.contains("from Shapes"), "hover text: {text}");
}

#[test]
fn hover_on_an_undocumented_local_value_is_the_type_alone() {
    let project = project("undocumented_local", DOCUMENTED_LIBRARY, ENTRY);
    let text = hover_text(&project.uri(), ENTRY, "describe unitShape", "unitShape");
    assert_eq!(text, "```emet\nShape\n```");
}
#[test]
fn document_symbols_list_the_entry_files_top_level_definitions() {
    let project = project("document_symbols", DOCUMENTED_LIBRARY, ENTRY);
    let symbols = emet_lsp::document_symbols(&project.uri(), ENTRY);
    let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["unitShape", "main"]);
    let unit_shape = &symbols[0];
    assert_eq!(unit_shape.detail.as_deref(), Some("Shape"));
    assert_eq!(
        unit_shape.selection_range.start.line,
        ENTRY
            .lines()
            .position(|l| l.starts_with("unitShape ="))
            .unwrap() as u32
    );
}

const DOCUMENTED_TYPE_LIBRARY: &str = "module Shapes exposing (Shape(..), describe)\n\n-- A shape, round or square.\ntype Shape = Circle Int | Square Int\n\ndescribe : Shape -> String\ndescribe shape =\n  case shape of\n    Circle _ ->\n      \"round\"\n    Square _ ->\n      \"boxy\"\n";

#[test]
fn hover_on_an_imported_type_name_carries_its_declaration_doc_and_origin() {
    let project = project("imported_type_hover", DOCUMENTED_TYPE_LIBRARY, ENTRY);
    let text = hover_text(&project.uri(), ENTRY, "unitShape : Shape", "Shape");
    assert!(
        text.contains("type Shape\n    = Circle Int\n    | Square Int"),
        "hover text: {text}"
    );
    assert!(
        text.contains("A shape, round or square."),
        "hover text: {text}"
    );
    assert!(text.contains("from Shapes"), "hover text: {text}");
}

#[test]
fn hover_on_a_builtin_type_name_in_an_annotation_names_the_type() {
    let project = project("builtin_type_hover", DOCUMENTED_TYPE_LIBRARY, ENTRY);
    let text = hover_text(&project.uri(), ENTRY, "main : List Scroll", "Scroll");
    assert_eq!(text, "```emet\ntype Scroll\n```");
}
