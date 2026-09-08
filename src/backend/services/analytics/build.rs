//! Gathers the shipped dataset declarations so the service carries them.
//!
//! A dataset declares itself beside the model that materializes it, under the
//! datasets folder — outside this crate. `DATASETS_DIR` points at that folder:
//! the image sets it to the copy taken from a named build context, and a local
//! build finds it in the repository.

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_DIR: &str = "../../../ingestion/datasets";

fn main() {
    let dir = datasets_dir();
    println!("cargo:rerun-if-env-changed=DATASETS_DIR");
    println!("cargo:rerun-if-changed={}", dir.display());

    let mut declarations: Vec<PathBuf> = read_dir(&dir)
        .map(|entry| entry.path().join("dataset.yaml"))
        .filter(|path| path.is_file())
        .collect();
    declarations.sort();
    assert!(
        !declarations.is_empty(),
        "no dataset declarations under {}",
        dir.display()
    );

    let mut generated = String::from("[\n");
    for path in &declarations {
        println!("cargo:rerun-if-changed={}", path.display());
        let _ = writeln!(generated, "    include_str!(r\"{}\"),", path.display());
    }
    generated.push(']');

    let out = Path::new(&var("OUT_DIR")).join("shipped_datasets.rs");
    fs::write(&out, generated).unwrap_or_else(|error| {
        panic!(
            "the generated declaration list is not writable at {}: {error}",
            out.display()
        )
    });
}

fn read_dir(dir: &Path) -> impl Iterator<Item = fs::DirEntry> {
    fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("datasets folder {} is unreadable: {error}", dir.display()))
        .filter_map(Result::ok)
}

fn datasets_dir() -> PathBuf {
    let raw = env::var("DATASETS_DIR").unwrap_or_else(|_| DEFAULT_DIR.to_owned());
    let path = PathBuf::from(&raw);
    if path.is_absolute() {
        return path;
    }
    PathBuf::from(var("CARGO_MANIFEST_DIR")).join(path)
}

fn var(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("cargo sets {name}"))
}
