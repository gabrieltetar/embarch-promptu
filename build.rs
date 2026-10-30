//! Embeds `patterns/*.md` in the binary: the public pattern set ships inside
//! `promptu`, so a run needs no files beside it (`PROMPTU_PATTERNS` points at
//! a folder instead, for development and lint).

use std::io::Write;

fn main() {
    let dir = std::path::Path::new("patterns");
    println!("cargo:rerun-if-changed=patterns");
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .expect("patterns/ exists")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .collect();
    files.sort();
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("patterns.rs");
    let mut f = std::fs::File::create(out).unwrap();
    writeln!(f, "pub const EMBEDDED: &[(&str, &str)] = &[").unwrap();
    for p in files {
        println!("cargo:rerun-if-changed={}", p.display());
        let abs = std::fs::canonicalize(&p).unwrap();
        let name = p.file_name().unwrap().to_string_lossy();
        writeln!(f, "    ({name:?}, include_str!({:?})),", abs.display().to_string()).unwrap();
    }
    writeln!(f, "];").unwrap();
}
