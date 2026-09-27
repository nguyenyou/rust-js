//! Versioned build-tool contract. Paths are resolved by artifact planning.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const VERSION: u32 = 1;
pub const ABI_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
pub struct Compiler {
    pub version: String,
    pub toolchain: String,
    pub abi: u32,
}

impl Compiler {
    pub fn current() -> Self {
        let value = |text: &str, key: &str| {
            text.lines()
                .find_map(|line| line.strip_prefix(key))
                .expect("version is declared in the checked-in configuration")
                .trim()
                .trim_matches('"')
                .to_owned()
        };
        Self {
            version: value(include_str!("../Cargo.toml"), "version = "),
            toolchain: value(include_str!("../rust-toolchain.toml"), "channel = "),
            abi: ABI_VERSION,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Manifest {
    pub version: u32,
    // Version-1 manifests written before identity was added remain readable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compiler: Option<Compiler>,
    pub input: PathBuf,
    pub output: PathBuf,
    pub sources: Vec<PathBuf>,
    pub modules: Vec<Module>,
    pub artifacts: Vec<Artifact>,
}

#[derive(Serialize, Deserialize)]
pub struct Module {
    pub module: Vec<String>,
    pub file: PathBuf,
    pub map: PathBuf,
    pub source: Option<PathBuf>,
    pub imports: Vec<PathBuf>,
}

#[derive(Serialize, Deserialize)]
pub struct Artifact {
    pub file: PathBuf,
    pub hash: String,
}

impl Manifest {
    pub fn read(bytes: &[u8]) -> Result<Self, String> {
        let manifest: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        if manifest.version != VERSION {
            return Err(format!(
                "unsupported manifest version {}; expected {VERSION}",
                manifest.version
            ));
        }
        Ok(manifest)
    }
}
