//! A library's contract with the crates that use it (ADR 0100): what each
//! item another crate can reach is in JS, and what a consumer can't work out
//! from the types alone. Versioned on its own, beside the host manifest.

use crate::manifest::{Compiler, Manifest};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub const VERSION: u32 = 2;

/// A function, method or impl method another crate can reach.
#[derive(Clone, Serialize, Deserialize)]
pub struct Item {
    /// rustc's `DefPathHash` of it, which a consumer finds it by.
    pub key: String,
    /// Its path, for messages: `models::User::validate`.
    pub rust_path: String,
    /// The module whose file it's in.
    pub module: Vec<String>,
    /// Its name in that file: the function's, or its type's object of
    /// methods (ADR 0047), `User` for `User.validate`.
    pub export: String,
    /// A method's name in that object.
    pub member: Option<String>,
    /// The type parameters it's given a `dropT` for (ADR 0098).
    pub drops: Vec<u32>,
}

#[derive(Serialize, Deserialize)]
pub struct Library {
    pub version: u32,
    pub name: String,
    /// rustc's hash of the crate its JS was made from, which a consumer
    /// checks against the metadata its rustc loads (ADR 0100).
    pub crate_hash: String,
    pub items: Vec<Item>,
    /// The keys of the trait impls whose methods are items: a consumer calls
    /// these, not what a derive would be.
    pub impls: Vec<String>,
    /// The libraries it was compiled against, whose manifests its consumers
    /// must have too: which traits get dictionaries, and which types drop,
    /// is decided by which crates are rust-js's.
    pub libraries: Vec<String>,
    pub inputs: Vec<crate::manifest::Artifact>,
}

/// An item as a consumer imports it: from a JS file, relative to its own
/// output, by name, and a method of what it names.
pub struct Imported {
    pub from: String,
    pub export: String,
    pub member: Option<String>,
    pub drops: Vec<u32>,
}

/// What one library gives its consumers.
#[derive(Default)]
pub struct Exports {
    pub crate_hash: String,
    pub items: HashMap<String, Imported>,
    pub impls: HashSet<String>,
}

#[derive(Default)]
pub struct Dependencies {
    pub libraries: HashMap<String, Exports>,
    pub inputs: Vec<std::path::PathBuf>,
}

impl Dependencies {
    /// Read and validate complete dependency artifacts before compilation starts.
    pub fn load(paths: &[std::path::PathBuf], output: &Path) -> Result<Self, String> {
        let mut result = Self::default();
        let mut needed: Vec<(String, String)> = Vec::new();
        let output = crate::output::absolute(output)?;
        let parent = output.parent().ok_or("output has no parent")?;
        for path in paths {
            result.inputs.push(crate::output::absolute(path)?);
            let manifest = Manifest::read(&std::fs::read(path).map_err(|e| e.to_string())?)?;
            result.inputs.extend(manifest.sources.iter().cloned());
            if manifest.compiler.as_ref() != Some(&Compiler::current()) {
                return Err(format!("incompatible dependency compiler identity: {}", path.display()));
            }
            let library = manifest.library.ok_or("dependency manifest has no library contract")?;
            if library.version != VERSION {
                return Err(format!(
                    "unsupported library ABI {}; expected {VERSION}",
                    library.version
                ));
            }
            if result.libraries.contains_key(&library.name) {
                return Err(format!("duplicate dependency crate name: {}", library.name));
            }
            needed.extend(
                library
                    .libraries
                    .iter()
                    .map(|used| (library.name.clone(), used.clone())),
            );
            for artifact in manifest.artifacts.iter().chain(&library.inputs) {
                result.inputs.push(artifact.file.clone());
                let bytes = std::fs::read(&artifact.file).map_err(|e| e.to_string())?;
                if crate::output::fingerprint(&bytes) != artifact.hash {
                    return Err(format!("dependency artifact changed: {}", artifact.file.display()));
                }
            }
            let mut exports = Exports {
                crate_hash: library.crate_hash,
                impls: library.impls.into_iter().collect(),
                ..Exports::default()
            };
            for item in library.items {
                let module = manifest
                    .modules
                    .iter()
                    .find(|m| m.module == item.module)
                    .ok_or("dependency export references a missing module")?;
                if !manifest.artifacts.iter().any(|a| a.file == module.file) {
                    return Err("dependency module is not a fingerprinted artifact".into());
                }
                let imported = Imported {
                    from: {
                        let relative = crate::output::relative(parent, &module.file);
                        if relative.starts_with("../") || relative.starts_with("./") {
                            relative
                        } else {
                            format!("./{relative}")
                        }
                    },
                    export: item.export,
                    member: item.member,
                    drops: item.drops,
                };
                if exports.items.insert(item.key, imported).is_some() {
                    return Err("duplicate dependency export".into());
                }
            }
            result.libraries.insert(library.name, exports);
        }
        if let Some((library, used)) = needed.iter().find(|(_, used)| !result.libraries.contains_key(used)) {
            return Err(format!(
                "`{library}` was compiled against `{used}`, a rust-js library: give its manifest too, with --dependency"
            ));
        }
        result.inputs.sort();
        result.inputs.dedup();
        Ok(result)
    }
}
