use std::path::{Path, PathBuf};

use serde::Deserialize;

const MANIFEST_FILENAME: &str = "emet.json";

#[derive(Deserialize)]
struct Manifest {
    #[serde(default)]
    #[serde(rename = "source-directories")]
    source_directories: Vec<String>,
}

pub struct SearchPath {
    directories: Vec<PathBuf>,
}

impl SearchPath {
    pub fn directories(&self) -> &[PathBuf] {
        &self.directories
    }
}

pub fn search_path_for(entry: &Path) -> SearchPath {
    let entry_dir = entry
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    let mut directories = vec![entry_dir.clone()];
    if let Some((manifest_dir, manifest)) = discover(&entry_dir) {
        for relative in manifest.source_directories {
            let library_dir = manifest_dir.join(relative);
            if !directories.contains(&library_dir) {
                directories.push(library_dir);
            }
        }
    }
    SearchPath { directories }
}

fn discover(start: &Path) -> Option<(PathBuf, Manifest)> {
    for dir in start.ancestors() {
        let candidate = dir.join(MANIFEST_FILENAME);
        if let Ok(text) = std::fs::read_to_string(&candidate) {
            if let Ok(manifest) = serde_json::from_str::<Manifest>(&text) {
                return Some((dir.to_path_buf(), manifest));
            }
        }
    }
    None
}
