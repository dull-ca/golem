//! Every example under `examples/` is documentation that readers copy into
//! their own files, so it must keep compiling. This suite discovers the
//! examples at test time (no hard-coded list) and compiles each one,
//! failing loudly with the file name and compile-error phase/message if any
//! example rots.

use emet::compile;

fn examples_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples")
}

fn example_paths() -> Vec<std::path::PathBuf> {
    let dir = examples_dir();
    let entries = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("failed to read examples dir {}: {e}", dir.display()));

    let mut paths: Vec<std::path::PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("emet"))
        .collect();

    assert!(
        !paths.is_empty(),
        "no *.emet files found under {} — examples dir is empty or unreadable",
        dir.display()
    );

    paths.sort();
    paths
}

#[test]
fn every_example_compiles() {
    for path in example_paths() {
        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));

        if let Err(err) = compile(&src) {
            panic!(
                "{} failed to compile at {:?}: {}",
                path.display(),
                err.phase,
                err.msg
            );
        }
    }
}
