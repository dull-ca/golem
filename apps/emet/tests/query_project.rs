use std::fs;

use emet::analyze_project;

fn byte_offset(src: &str, needle: &str, occurrence: usize) -> usize {
    let mut start = 0;
    for _ in 0..occurrence {
        let found = src[start..].find(needle).expect("needle present");
        start += found + needle.len();
    }
    let found = src[start..].find(needle).expect("needle present");
    start + found
}

#[test]
fn imported_name_resolves_to_definition_span_and_module() {
    let dir = std::env::temp_dir().join(format!("emet_query_project_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let lib = "module Greeting exposing (hello)\nhello : String\nhello = \"hi\"\n";
    let main_src =
        "import Greeting\nmain : List Scroll\nmain =\n  let _unused = Greeting.hello\n  in []\n";

    fs::write(dir.join("Greeting.emet"), lib).unwrap();
    let entry = dir.join("Main.emet");
    fs::write(&entry, main_src).unwrap();

    let project = analyze_project(&entry);
    assert!(
        project.diagnostics.is_empty(),
        "no diagnostics: {:?}",
        project
            .diagnostics
            .iter()
            .map(|d| &d.msg)
            .collect::<Vec<_>>()
    );

    let entry_index = project.index_for(&entry).expect("entry module index");
    let use_offset = byte_offset(main_src, "Greeting.hello", 0);
    let def = entry_index
        .definition_at(use_offset)
        .expect("a definition site for the imported use");

    assert_eq!(def.module.as_deref(), Some("Greeting"));
    let binding_offset = byte_offset(lib, "hello", 2);
    assert_eq!(
        def.span.start, binding_offset,
        "def span {:?} starts at the binding of hello",
        def.span
    );

    let _ = fs::remove_dir_all(&dir);
}
