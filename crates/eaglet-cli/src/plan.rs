//! Every file a command creates or changes, collected first and written at the end, so a command
//! that fails halfway writes nothing and `--dry-run` can show the whole plan.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// One file to write.
#[derive(Debug, Clone)]
struct Change {
    content: String,
    created: bool,
}

/// The files a command will create or change, relative to the project root.
#[derive(Debug)]
pub struct Plan {
    root: PathBuf,
    changes: BTreeMap<PathBuf, Change>,
}

impl Plan {
    /// A plan for the project at `root`.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            changes: BTreeMap::new(),
        }
    }

    /// The project root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Whether `rel` exists, on disk or earlier in this plan.
    #[must_use]
    pub fn exists(&self, rel: &str) -> bool {
        self.changes.contains_key(Path::new(rel)) || self.root.join(rel).exists()
    }

    /// The current text of `rel`: as planned, or as on disk.
    ///
    /// # Errors
    /// When the file cannot be read.
    pub fn read(&self, rel: &str) -> Result<String, String> {
        if let Some(change) = self.changes.get(Path::new(rel)) {
            return Ok(change.content.clone());
        }
        std::fs::read_to_string(self.root.join(rel)).map_err(|e| format!("cannot read {rel}: {e}"))
    }

    /// Plan a new file.
    ///
    /// # Errors
    /// When the file already exists; eaglet never overwrites your code.
    pub fn create(&mut self, rel: &str, content: String) -> Result<(), String> {
        if self.exists(rel) {
            return Err(format!(
                "{rel} already exists; eaglet does not overwrite files"
            ));
        }
        self.changes.insert(
            PathBuf::from(rel),
            Change {
                content,
                created: true,
            },
        );
        Ok(())
    }

    /// Plan a change to a file; nothing is planned when the text is the same.
    ///
    /// # Errors
    /// When the file cannot be read.
    pub fn update(&mut self, rel: &str, content: String) -> Result<(), String> {
        if self.read(rel)? == content {
            return Ok(());
        }
        let created = self.changes.get(Path::new(rel)).is_some_and(|c| c.created);
        self.changes
            .insert(PathBuf::from(rel), Change { content, created });
        Ok(())
    }

    /// One line per file, like `create src/products/mod.rs`.
    #[must_use]
    pub fn summary(&self) -> Vec<String> {
        self.changes
            .iter()
            .map(|(path, c)| {
                let verb = if c.created { "create" } else { "update" };
                format!("{verb} {}", path.display())
            })
            .collect()
    }

    /// Write every file, then format the Rust ones with `rustfmt` when it is installed.
    ///
    /// # Errors
    /// When a file cannot be written.
    pub fn write(&self) -> Result<Vec<String>, String> {
        let mut rust_files = Vec::new();
        for (rel, change) in &self.changes {
            let path = self.root.join(rel);
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)
                    .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
            }
            std::fs::write(&path, &change.content)
                .map_err(|e| format!("cannot write {}: {e}", rel.display()))?;
            if rel.extension().is_some_and(|e| e == "rs") {
                rust_files.push(path);
            }
        }
        let mut notes = Vec::new();
        if !rust_files.is_empty() {
            match Command::new("rustfmt")
                .args(["--edition", "2024"])
                .args(&rust_files)
                .output()
            {
                Ok(out) if out.status.success() => {}
                Ok(out) => notes.push(format!(
                    "rustfmt could not format the files (run `cargo fmt` after fixing): {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                )),
                Err(_) => notes.push("rustfmt is not installed; run `cargo fmt` later".to_owned()),
            }
        }
        Ok(notes)
    }
}
