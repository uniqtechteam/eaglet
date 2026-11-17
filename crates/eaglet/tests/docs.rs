//! Every Rust block in `README.md` and `docs/*.md` is copied from code that this workspace builds
//! and tests, so the docs cannot drift from the code without this test failing.
//!
//! A block matches when its lines, with indentation and blank lines ignored, appear in that order
//! and next to each other in one of the source files. A line `// ...` in a block skips code: the
//! parts before and after it are matched separately, each against the same file. Mermaid diagrams
//! must be copies of a file in `docs/diagrams`, where they are kept for review and rendering.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "tests: a failure should stop the test"
)]

use std::fs;
use std::path::{Path, PathBuf};

/// The files snippets may come from, relative to the workspace root.
const SOURCES: &[&str] = &[
    "crates/eaglet/tests",
    "examples/shop/src",
    "examples/shop/tests",
    "crates/eaglet-cli/templates",
];

/// The documents whose Rust blocks are checked.
const DOCS: &[&str] = &["README.md", "docs"];

/// The workspace root, or `None` when the crate is built from its crates.io package, which has
/// neither the docs nor the example to check them against.
fn root() -> Option<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    if root.join("docs/diagrams").is_dir() {
        Some(root)
    } else {
        eprintln!("not in the eaglet repository, nothing to check");
        None
    }
}

fn files(dir: &Path, ext: &[&str], out: &mut Vec<PathBuf>) {
    if dir.is_file() {
        out.push(dir.to_path_buf());
        return;
    }
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files(&path, ext, out);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| ext.contains(&e))
        {
            out.push(path);
        }
    }
}

/// Trimmed, non-empty lines.
fn lines(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect()
}

/// The blocks of a Markdown file fenced as ```` ```lang ````, with the line each starts on.
fn blocks(markdown: &str, lang: &str) -> Vec<(usize, String)> {
    let open = format!("```{lang}");
    let mut blocks = Vec::new();
    let mut current: Option<(usize, String)> = None;
    for (n, line) in markdown.lines().enumerate() {
        let fence = line.trim_start();
        match &mut current {
            None if fence.starts_with(&open) => current = Some((n + 1, String::new())),
            Some(_) if fence.starts_with("```") => blocks.extend(current.take()),
            Some((_, body)) => {
                body.push_str(line);
                body.push('\n');
            }
            None => {}
        }
    }
    blocks
}

fn contains(haystack: &[&str], needle: &[&str]) -> bool {
    needle.is_empty() || haystack.windows(needle.len()).any(|w| w == needle)
}

/// Does `block` match `source`, with `// ...` skipping code?
fn matches(source: &[&str], block: &str) -> bool {
    let block = lines(block);
    block
        .split(|l| *l == "// ...")
        .all(|part| contains(source, part))
}

#[test]
fn every_rust_block_in_the_docs_is_real_code() {
    let Some(root) = root() else { return };
    let mut sources = Vec::new();
    for dir in SOURCES {
        files(&root.join(dir), &["rs", "tmpl"], &mut sources);
    }
    let sources: Vec<(PathBuf, String)> = sources
        .into_iter()
        .map(|p| {
            let text = fs::read_to_string(&p).unwrap();
            (p, text)
        })
        .collect();
    let source_lines: Vec<Vec<&str>> = sources.iter().map(|(_, t)| lines(t)).collect();

    let mut docs = Vec::new();
    for doc in DOCS {
        files(&root.join(doc), &["md"], &mut docs);
    }
    let mut checked = 0;
    let mut missing = Vec::new();
    for doc in &docs {
        let text = fs::read_to_string(doc).unwrap();
        for (line, block) in blocks(&text, "rust") {
            checked += 1;
            if !source_lines.iter().any(|src| matches(src, &block)) {
                let name = doc.strip_prefix(&root).unwrap_or(doc).display();
                missing.push(format!("{name}:{line}"));
            }
        }
    }
    assert!(
        checked >= 20,
        "only {checked} Rust blocks found in the docs"
    );
    assert!(
        missing.is_empty(),
        "these Rust blocks are not copied from code that builds (see tests/docs.rs): {missing:?}"
    );
}

#[test]
fn every_diagram_in_the_docs_is_one_of_docs_diagrams() {
    let Some(root) = root() else { return };
    let mut diagrams = Vec::new();
    files(&root.join("docs/diagrams"), &["mmd"], &mut diagrams);
    let diagrams: Vec<String> = diagrams
        .iter()
        .map(|p| fs::read_to_string(p).unwrap().trim().to_owned())
        .collect();
    let mut docs = Vec::new();
    for doc in DOCS {
        files(&root.join(doc), &["md"], &mut docs);
    }
    for doc in &docs {
        let text = fs::read_to_string(doc).unwrap();
        for (line, block) in blocks(&text, "mermaid") {
            assert!(
                diagrams.iter().any(|d| *d == block.trim()),
                "{}:{line}: this diagram differs from every file in docs/diagrams",
                doc.display()
            );
        }
    }
}

#[test]
fn a_changed_block_is_caught() {
    let source = lines("fn a() {\n    one();\n\n    two();\n}\n");
    assert!(matches(&source, "fn a() {\n one();\n two();\n}"));
    assert!(matches(&source, "fn a() {\n// ...\n    two();"));
    assert!(!matches(&source, "fn a() {\n    two();\n}"));
    assert!(!matches(&source, "fn a() {\n    three();\n}"));
}
