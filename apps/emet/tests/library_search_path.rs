use std::fs;
use std::path::PathBuf;

use emet::{compile_file, compile_file_all, Phase};

struct Project {
    root: PathBuf,
}

impl Project {
    fn new(tag: &str) -> Project {
        let root = std::env::temp_dir().join(format!("emet_libpath_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        Project { root }
    }

    fn write(&self, rel: &str, contents: &str) -> PathBuf {
        let path = self.root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, contents).unwrap();
        path
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn scroll_keys(c: &emet::Compiled) -> Vec<String> {
    c.scrolls
        .iter()
        .flat_map(|s| s.glyphs().iter().map(|g| g.key()))
        .collect()
}

#[test]
fn library_in_search_path_imports_from_a_different_directory() {
    let p = Project::new("crossdir");
    p.write("emet.json", r#"{ "source-directories": ["lib"] }"#);
    p.write(
        "lib/Pkgs.emet",
        "module Pkgs exposing (web)\nweb : AptPackage\nweb = aptPackage { name = \"nginx\" }\n",
    );
    let entry = p.write(
        "app/Main.emet",
        "import Pkgs\nmain : List Scroll\nmain = [ scroll { name = \"web\", glyphs = [ Pkgs.web ] } ]\n",
    );

    let c = compile_file(&entry).expect("entry imports a library from a sibling lib dir");
    assert_eq!(scroll_keys(&c), vec!["apt:nginx".to_string()]);
}

#[test]
fn exposing_across_the_search_path_brings_name_unqualified() {
    let p = Project::new("exposing");
    p.write("emet.json", r#"{ "source-directories": ["lib"] }"#);
    p.write(
        "lib/Pkgs.emet",
        "module Pkgs exposing (web)\nweb : AptPackage\nweb = aptPackage { name = \"nginx\" }\n",
    );
    let entry = p.write(
        "app/Main.emet",
        "import Pkgs exposing (web)\nmain : List Scroll\nmain = [ scroll { name = \"web\", glyphs = [ web ] } ]\n",
    );

    let c = compile_file(&entry).expect("exposing across the search path resolves");
    assert_eq!(scroll_keys(&c), vec!["apt:nginx".to_string()]);
}

#[test]
fn entry_directory_wins_over_a_library_of_the_same_name() {
    let p = Project::new("precedence");
    p.write("emet.json", r#"{ "source-directories": ["lib"] }"#);
    p.write(
        "lib/Pkgs.emet",
        "module Pkgs exposing (web)\nweb : AptPackage\nweb = aptPackage { name = \"library\" }\n",
    );
    p.write(
        "app/Pkgs.emet",
        "module Pkgs exposing (web)\nweb : AptPackage\nweb = aptPackage { name = \"entrylocal\" }\n",
    );
    let entry = p.write(
        "app/Main.emet",
        "import Pkgs\nmain : List Scroll\nmain = [ scroll { name = \"web\", glyphs = [ Pkgs.web ] } ]\n",
    );

    let c = compile_file(&entry).expect("entry-dir module shadows the library");
    assert_eq!(scroll_keys(&c), vec!["apt:entrylocal".to_string()]);
}

#[test]
fn a_cycle_across_the_search_path_is_rejected() {
    let p = Project::new("cycle");
    p.write("emet.json", r#"{ "source-directories": ["lib"] }"#);
    p.write(
        "lib/Ring.emet",
        "module Ring exposing (pkg)\nimport Main\npkg : AptPackage\npkg = aptPackage { name = \"ring\" }\n",
    );
    let entry = p.write(
        "app/Main.emet",
        "module Main exposing (..)\nimport Ring\nmain : List Scroll\nmain = [ scroll { name = \"a\", glyphs = [ Ring.pkg ] } ]\n",
    );

    let err = compile_file(&entry).expect_err("a cycle spanning the search path must be rejected");
    assert!(
        err.msg.contains("cycle"),
        "expected a cycle diagnostic, got: {}",
        err.msg
    );
}

#[test]
fn manifest_is_discovered_by_walking_up_from_the_entry() {
    let p = Project::new("walkup");
    p.write("emet.json", r#"{ "source-directories": ["lib"] }"#);
    p.write(
        "lib/Pkgs.emet",
        "module Pkgs exposing (web)\nweb : AptPackage\nweb = aptPackage { name = \"nginx\" }\n",
    );
    let entry = p.write(
        "app/nested/deep/Main.emet",
        "import Pkgs\nmain : List Scroll\nmain = [ scroll { name = \"web\", glyphs = [ Pkgs.web ] } ]\n",
    );

    let c = compile_file(&entry).expect("manifest found by walking up several dirs");
    assert_eq!(scroll_keys(&c), vec!["apt:nginx".to_string()]);
}

#[test]
fn without_a_manifest_only_the_entry_directory_resolves() {
    let p = Project::new("nomanifest");
    p.write(
        "lib/Pkgs.emet",
        "module Pkgs exposing (web)\nweb : AptPackage\nweb = aptPackage { name = \"nginx\" }\n",
    );
    let entry = p.write(
        "app/Main.emet",
        "import Pkgs\nmain : List Scroll\nmain = [ scroll { name = \"web\", glyphs = [ Pkgs.web ] } ]\n",
    );

    let err = compile_file(&entry)
        .expect_err("with no manifest, a library outside the entry dir is not found");
    assert_eq!(err.phase, Phase::Parse);
    assert!(
        err.msg.contains("cannot find imported module"),
        "expected a missing-module diagnostic, got: {}",
        err.msg
    );
}

#[test]
fn parse_error_msg_has_no_filename_prefix() {
    let p = Project::new("noprefix");
    let entry = p.write("Main.emet", "main = [ \"a\" ");

    let errors = compile_file_all(&entry).expect_err("unclosed bracket is a parse error");
    let e = &errors[0];
    assert!(!e.msg.contains(".emet:"), "filename leaked into label: {}", e.msg);
    assert!(
        !e.msg.contains(entry.to_str().unwrap()),
        "path leaked: {}",
        e.msg
    );
}

#[test]
fn parse_error_identifies_the_imported_module() {
    let p = Project::new("importedparse");
    p.write("emet.json", r#"{ "source-directories": ["lib"] }"#);
    p.write("lib/Pkgs.emet", "module Pkgs exposing (..)\n\nbroken = [ \"a\" \n");
    let entry = p.write(
        "app/Main.emet",
        "import Pkgs\n\nmain = [ scroll { name = \"h\", glyphs = [] } ]\n",
    );

    let errors = compile_file_all(&entry).expect_err("imported module has an unclosed bracket");
    let e = &errors[0];
    assert_eq!(e.phase, Phase::Parse, "an unclosed bracket is a parse error");
    let file = e
        .file
        .as_ref()
        .expect("a parse error in an imported module names its file");
    assert!(
        file.ends_with("Pkgs.emet"),
        "error should point at the imported module, got {}",
        file.display()
    );
    assert_ne!(
        file, &entry,
        "error must not be attributed to the entry file"
    );
}
