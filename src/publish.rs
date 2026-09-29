//! Publish a complete artifact plan. No frontend, lowering or printing APIs.

use std::path::{Path, PathBuf};

pub(crate) struct Artifact {
    pub(crate) path: PathBuf,
    /// JS, a map, a manifest, or a library's metadata (ADR 0100).
    pub(crate) bytes: Vec<u8>,
}

/// Constructed only after all names and contents have been validated.
pub(crate) struct ArtifactPlan {
    pub(crate) artifacts: Vec<Artifact>,
    pub(crate) stale: Vec<PathBuf>,
}

impl ArtifactPlan {
    pub(crate) fn publish(self) -> Result<(), String> {
        publish(&self.artifacts, &self.stale)
    }
}

fn parent_dir(path: &Path) -> &Path {
    path.parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn publish(artifacts: &[Artifact], stale: &[PathBuf]) -> Result<(), String> {
    // WASI compiles into a fresh, disposable virtual filesystem. The browser
    // shim does not provide native rename semantics; callers expose files only
    // after success. Validation and generation above still finish before writes.
    if cfg!(target_os = "wasi") {
        for a in artifacts {
            std::fs::create_dir_all(parent_dir(&a.path)).map_err(|e| e.to_string())?;
            std::fs::write(&a.path, &a.bytes).map_err(|e| e.to_string())?;
        }
        return Ok(());
    }
    let artifacts: Vec<&Artifact> = artifacts
        .iter()
        .filter(|a| !std::fs::read(&a.path).is_ok_and(|bytes| bytes == a.bytes))
        .collect();
    // Stage every file next to its destination before replacing any output.
    // Keep originals for rollback on an I/O error. This is not a multi-file
    // transaction against process crashes; the manifest is committed last.
    let mut staged = Vec::new();
    let result = (|| -> Result<(), String> {
        for a in &artifacts {
            let dir = parent_dir(&a.path);
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            let mut number = 0;
            let temp = loop {
                let candidate = dir.join(format!(".rust-js-stage-{number}"));
                match std::fs::create_dir(&candidate) {
                    Ok(()) => break candidate,
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => number += 1,
                    Err(e) => return Err(format!("cannot stage in {}: {e}", dir.display())),
                }
            };
            staged.push(temp.clone());
            std::fs::write(temp.join("next"), &a.bytes).map_err(|e| e.to_string())?;
        }
        let originals: Vec<(PathBuf, Option<Vec<u8>>)> = artifacts
            .iter()
            .map(|a| &a.path)
            .chain(stale)
            .map(|path| {
                let bytes = match std::fs::read(path) {
                    Ok(bytes) => Some(bytes),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                    Err(e) => return Err(e.to_string()),
                };
                Ok((path.clone(), bytes))
            })
            .collect::<Result<_, String>>()?;
        let commit = (|| -> std::io::Result<()> {
            // Publish the manifest last, after removal of obsolete artifacts.
            for path in stale {
                std::fs::remove_file(path)?;
            }
            for (a, temp) in artifacts.iter().zip(&staged) {
                std::fs::rename(temp.join("next"), &a.path)?;
            }
            Ok(())
        })();
        if let Err(error) = commit {
            let mut failures = Vec::new();
            for (path, bytes) in originals {
                let restored = match bytes {
                    Some(bytes) => std::fs::write(&path, bytes),
                    None if path.exists() => std::fs::remove_file(&path),
                    None => Ok(()),
                };
                if let Err(e) = restored {
                    failures.push(format!("{}: {e}", path.display()));
                }
            }
            return Err(format!(
                "cannot publish output: {error}; rollback errors: {}",
                failures.join(", ")
            ));
        }
        Ok(())
    })();
    for dir in staged {
        let _ = std::fs::remove_dir_all(dir);
    }
    result
}
