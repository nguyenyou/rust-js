//! A crate's settings for the JS rust-js writes (ADR 0117), in its
//! `Cargo.toml`'s `[package.metadata.rust-js]`: the options of the formatter
//! it has, as oxfmt names them, and the hooks it runs. What they are to oxc
//! is `format`'s to say.

use std::path::PathBuf;

use serde::Deserialize;

#[cfg(not(target_os = "wasi"))]
pub use cargo_toml::read;

/// What a crate's settings are: none, but for a crate that says.
#[derive(Default)]
pub struct Settings {
    pub format: Format,
    pub hooks: Hooks,
    /// Where its `Cargo.toml` is, which its hooks are run in, and what's
    /// said of a file is said from.
    pub dir: PathBuf,
}

/// `[package.metadata.rust-js.format]`: oxfmt's options, by its names,
/// Prettier's, over rust-js's own. None, rust-js's layout.
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Format {
    pub print_width: Option<u16>,
    pub tab_width: Option<u8>,
    pub use_tabs: Option<bool>,
    pub end_of_line: Option<EndOfLine>,
    pub single_quote: Option<bool>,
    pub jsx_single_quote: Option<bool>,
    pub quote_props: Option<QuoteProps>,
    pub trailing_comma: Option<TrailingComma>,
    pub semi: Option<bool>,
    pub arrow_parens: Option<ArrowParens>,
    pub bracket_spacing: Option<bool>,
    pub bracket_same_line: Option<bool>,
    pub single_attribute_per_line: Option<bool>,
    pub object_wrap: Option<ObjectWrap>,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum EndOfLine {
    Lf,
    Crlf,
    Cr,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum QuoteProps {
    AsNeeded,
    Consistent,
    Preserve,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum TrailingComma {
    All,
    Es5,
    None,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum ArrowParens {
    Always,
    Avoid,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum ObjectWrap {
    Preserve,
    Collapse,
}

/// `[package.metadata.rust-js.hooks]`.
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hooks {
    /// Commands a module's text goes through, in order, each one's stdin
    /// the text, its stdout the text it gives.
    #[serde(default)]
    pub transform: Vec<Vec<String>>,
    /// Commands run on what a build wrote.
    #[serde(default)]
    pub check: Vec<Check>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Check {
    pub run: Vec<String>,
    #[serde(default)]
    pub fail: Fail,
    #[serde(default)]
    pub when: When,
}

/// What a check's failure, an exit otherwise than 0, is.
#[derive(Default, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Fail {
    /// The build's.
    #[default]
    Build,
    /// Its output, as warnings.
    Never,
}

/// Which builds run a check.
#[derive(Default, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum When {
    /// Each one, a save in the Vite server too.
    #[default]
    Always,
    /// A build, not a save.
    Build,
}

/// Reading them from a `Cargo.toml`, natively: the playground has none.
#[cfg(not(target_os = "wasi"))]
mod cargo_toml {
    use std::path::{Path, PathBuf};

    use serde::Deserialize;

    use super::{Format, Hooks, Settings};

    /// `[package.metadata.rust-js]`, of which these are two tables: ADR
    /// 0116's are others.
    #[derive(Default, Deserialize)]
    struct RustJs {
        #[serde(default)]
        format: Format,
        #[serde(default)]
        hooks: Hooks,
    }

    #[derive(Deserialize)]
    struct Manifest {
        package: Option<Package>,
    }

    #[derive(Deserialize)]
    struct Package {
        metadata: Option<Metadata>,
    }

    #[derive(Deserialize)]
    struct Metadata {
        #[serde(rename = "rust-js")]
        rust_js: Option<RustJs>,
    }

    /// The settings of the crate whose root is `input`: its `Cargo.toml`'s,
    /// Cargo's for a crate it builds, `CARGO_MANIFEST_DIR`, or else the
    /// nearest above it. None without one.
    pub fn read(input: &Path, cargo: bool) -> Result<Settings, String> {
        let dir = if cargo {
            std::env::var_os("CARGO_MANIFEST_DIR").map(PathBuf::from)
        } else {
            let input = crate::output::absolute(input)?;
            input
                .ancestors()
                .skip(1)
                .find(|dir| dir.join("Cargo.toml").is_file())
                .map(PathBuf::from)
        };
        let Some(dir) = dir else {
            return Ok(Settings::default());
        };
        let path = dir.join("Cargo.toml");
        let text = std::fs::read_to_string(&path).map_err(|e| format!("cannot read `{}`: {e}", path.display()))?;
        let manifest: Manifest = toml::from_str(&text).map_err(|e| format!("`{}`: {e}", path.display()))?;
        let settings = manifest
            .package
            .and_then(|package| package.metadata)
            .and_then(|metadata| metadata.rust_js)
            .unwrap_or_default();
        // A width oxfmt can't take fails here, once, not each module after.
        crate::format::options_of(&settings.format).map_err(|e| format!("`{}`: {e}", path.display()))?;
        Ok(Settings {
            format: settings.format,
            hooks: settings.hooks,
            dir,
        })
    }
}

#[cfg(target_os = "wasi")]
pub fn read(_: &std::path::Path, _: bool) -> Result<Settings, String> {
    Ok(Settings::default())
}
