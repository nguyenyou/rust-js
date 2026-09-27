//! Experimental scalar-library linkage contract. Versioned independently of the
//! host manifest: this is deliberately not an ABI for traits or arbitrary types.

use crate::manifest::{Compiler, Manifest};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

pub const VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scalar {
    Bool,
    I32,
    U32,
    Unit,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Signature {
    pub inputs: Vec<Scalar>,
    pub output: Scalar,
}

#[derive(Serialize, Deserialize)]
pub struct Function {
    pub rust_path: String,
    pub module: Vec<String>,
    pub export: String,
    pub signature: Signature,
}

#[derive(Serialize, Deserialize)]
pub struct Library {
    pub version: u32,
    pub name: String,
    pub functions: Vec<Function>,
    pub inputs: Vec<crate::manifest::Artifact>,
}

pub struct ImportedFunction {
    pub from: String,
    pub export: String,
    pub signature: Signature,
}

#[derive(Default)]
pub struct Dependencies {
    pub libraries: HashMap<String, HashMap<String, ImportedFunction>>,
    pub inputs: Vec<std::path::PathBuf>,
}

impl Dependencies {
    /// Read and validate complete dependency artifacts before compilation starts.
    pub fn load(paths: &[std::path::PathBuf], output: &Path) -> Result<Self, String> {
        let mut result = Self::default();
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
            for artifact in manifest.artifacts.iter().chain(&library.inputs) {
                result.inputs.push(artifact.file.clone());
                let bytes = std::fs::read(&artifact.file).map_err(|e| e.to_string())?;
                if crate::output::fingerprint(&bytes) != artifact.hash {
                    return Err(format!("dependency artifact changed: {}", artifact.file.display()));
                }
            }
            let mut functions = HashMap::new();
            for function in library.functions {
                let module = manifest
                    .modules
                    .iter()
                    .find(|m| m.module == function.module)
                    .ok_or("dependency export references a missing module")?;
                if !manifest.artifacts.iter().any(|a| a.file == module.file) {
                    return Err("dependency module is not a fingerprinted artifact".into());
                }
                let imported = ImportedFunction {
                    from: {
                        let relative = crate::output::relative(parent, &module.file);
                        if relative.starts_with("../") || relative.starts_with("./") {
                            relative
                        } else {
                            format!("./{relative}")
                        }
                    },
                    export: function.export,
                    signature: function.signature,
                };
                if functions.insert(function.rust_path, imported).is_some() {
                    return Err("duplicate dependency export".into());
                }
            }
            result.libraries.insert(library.name, functions);
        }
        result.inputs.sort();
        result.inputs.dedup();
        Ok(result)
    }
}
