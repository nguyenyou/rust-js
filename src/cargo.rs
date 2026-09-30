//! rust-js as Cargo's `RUSTC_WORKSPACE_WRAPPER` (ADR 0101). Cargo runs it
//! with rustc's path and the flags it worked out, for each member of the
//! workspace it builds, and for its probes of rustc. A library of the
//! workspace, checked for rust-js's target, is compiled by rust-js: its
//! metadata where Cargo expects it (ADR 0100), and its JS and manifest beside
//! it, in `rust-js/<crate>-<hash>/`, one for each build of it Cargo keeps.
//! Anything else, a probe, a build script, a procedural macro, a native
//! build, is rustc's, as Cargo asked.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// What running rust-js as it was run comes to.
pub enum Invocation {
    /// rust-js's own arguments: as given, or as Cargo's flags make them.
    RustJs(Vec<String>),
    /// rustc was run as Cargo asked, and ended so.
    Rustc(ExitCode),
}

/// rust-js's target (ADR 0090): what a crate Cargo checks for it is built for.
const TARGET: &str = "wasm32-unknown-unknown";

/// The packages of the bindings rust-js ships: `react/`, `webapi/` and `builtins/`.
const BINDINGS: [&str; 3] = ["rust-js-react", "rust-js-webapi", "rust-js-builtins"];

pub fn translate(args: Vec<String>) -> Invocation {
    let Some(rustc) = args
        .first()
        .filter(|arg| Path::new(arg).file_stem().is_some_and(|stem| stem == "rustc"))
        .cloned()
    else {
        return Invocation::RustJs(args);
    };
    let given = &args[1..];
    let flags = match expand(given) {
        Ok(flags) => flags,
        Err(error) => {
            eprintln!("rust-js: {error}");
            return Invocation::Rustc(ExitCode::FAILURE);
        }
    };
    let library = values(&flags, "--crate-type").any(|kind| kind == "lib" || kind == "rlib");
    let probe = flags
        .iter()
        .any(|flag| flag == "-" || flag == "-vV" || flag.starts_with("--print"));
    // rust-js's bindings are metadata only (ADR 0024), rustc's to check, a
    // member of the workspace or not: installed under an app's
    // `node_modules`, they're in its workspace. rustc is rust-js's `--rustc`,
    // with its tool known, as the tooling runs Cargo (ADR 0112).
    let bindings = std::env::var("CARGO_PKG_NAME").is_ok_and(|name| BINDINGS.contains(&name.as_str()));
    if probe || bindings || !library || values(&flags, "--target").next().as_deref() != Some(TARGET) {
        return rustc_itself(&rustc, given);
    }
    match compile(&flags) {
        Ok(args) => Invocation::RustJs(args),
        Err(error) => {
            eprintln!("rust-js: {error}");
            Invocation::Rustc(ExitCode::FAILURE)
        }
    }
}

/// rust-js's arguments for a library Cargo checks for its target.
fn compile(flags: &[String]) -> Result<Vec<String>, String> {
    let name = values(flags, "--crate-name")
        .next()
        .ok_or("Cargo gave no --crate-name")?;
    let source = flags
        .iter()
        .find(|flag| flag.ends_with(".rs"))
        .ok_or("Cargo gave no source file")?
        .clone();
    let out_dir = PathBuf::from(values(flags, "--out-dir").next().ok_or("Cargo gave no --out-dir")?);
    let extra = values(flags, "-C")
        .find_map(|option| option.strip_prefix("extra-filename=").map(str::to_string))
        .unwrap_or_default();
    let emitted: Vec<String> = values(flags, "--emit")
        .flat_map(|kinds| kinds.split(',').map(str::to_string).collect::<Vec<_>>())
        .collect();
    // What `cargo build` asks for, rustc links. Only a check's crates are
    // metadata alone, which Cargo never starts a dependent before rustc ends:
    // rust-js publishes a crate's with its JS, after.
    if emitted.iter().any(|kind| kind == "link") {
        return Err("rust-js writes JS for `cargo check`; `cargo build` asks for what rustc links".into());
    }
    // Each build of the crate Cargo keeps, of a feature set say, has JS of its
    // own, as it has metadata, so a build Cargo has as done is still its JS.
    let dir = out_dir.join("rust-js").join(format!("{name}{extra}"));
    let mut ours = vec![
        source.clone(),
        "-o".into(),
        dir.join("lib.js").display().to_string(),
        "--manifest".into(),
        dir.join("lib.manifest.json").display().to_string(),
        "--cargo".into(),
        // Each is a library, the one Cargo was asked for too: built as the
        // app, it would be fresh when another build uses it, and Cargo can't
        // be told which it was, as it drops what it set for rustc itself,
        // `CARGO_PRIMARY_PACKAGE`, from what it tracks.
        "--library".into(),
    ];
    for manifest in dependencies(flags)? {
        ours.push("--dependency".into());
        ours.push(manifest.display().to_string());
    }
    ours.push("--".into());
    // What rustc writes is what Cargo expects, where it expects it: the
    // metadata, published with the JS, and Cargo's own record of the sources.
    let mut emit = vec![format!(
        "metadata={}",
        out_dir.join(format!("lib{name}{extra}.rmeta")).display()
    )];
    if emitted.iter().any(|kind| kind == "dep-info") {
        emit.push(format!(
            "dep-info={}",
            out_dir.join(format!("{name}{extra}.d")).display()
        ));
    }
    ours.push(format!("--emit={}", emit.join(",")));
    let mut rest = flags.iter();
    while let Some(flag) = rest.next() {
        if *flag == source {
            continue;
        }
        if flag == "--emit" {
            rest.next();
            continue;
        }
        if flag.starts_with("--emit=") {
            continue;
        }
        ours.push(flag.clone());
    }
    Ok(ours)
}

/// The manifests of the libraries of the workspace this crate uses, directly
/// or through one of them. Beside the metadata of each is its manifest, and
/// the manifests it was compiled with (`record`), of the builds Cargo gave
/// it; a dependency without that, serde say, is a crate rustc built. One with
/// it and a manifest gone is refused: a crate that uses it would be compiled
/// as if it were rustc's.
fn dependencies(flags: &[String]) -> Result<Vec<PathBuf>, String> {
    let mut found: Vec<PathBuf> = values(flags, "--extern")
        .filter_map(|given| Some(PathBuf::from(given.split_once('=')?.1)))
        .filter_map(|metadata| std::fs::read_to_string(marker(&metadata)).ok())
        .flat_map(|recorded| recorded.lines().map(PathBuf::from).collect::<Vec<_>>())
        .collect();
    found.sort();
    found.dedup();
    if let Some(gone) = found.iter().find(|manifest| !manifest.is_file()) {
        return Err(format!(
            "{} is gone; rust-js wrote it for Cargo's build, which has it as done: `cargo clean` to build it again",
            gone.display()
        ));
    }
    Ok(found)
}

/// What Cargo is told of a crate rust-js compiled (ADR 0101), each file as
/// it's published with the JS, or not at all: beside its metadata, where its
/// manifest is, and those of the libraries it was compiled with, `used`, for
/// the crates that use it; and Cargo's record of the
/// sources, as rustc wrote it to `dep_info`'s first path, with rust-js
/// itself added to the files, or Cargo would have the crate as done when
/// rust-js had changed.
pub fn record(
    manifest: &Path,
    used: &[PathBuf],
    metadata: &Path,
    dep_info: Option<(&Path, &Path)>,
) -> Result<Vec<(PathBuf, Vec<u8>)>, String> {
    let mut manifests = Vec::new();
    for manifest in std::iter::once(manifest).chain(used.iter().map(PathBuf::as_path)) {
        let path = std::path::absolute(manifest).map_err(|e| format!("{}: {e}", manifest.display()))?;
        manifests.push(format!("{}\n", path.display()));
    }
    let mut records = vec![(marker(metadata), manifests.concat().into_bytes())];
    let Some((written, path)) = dep_info else {
        return Ok(records);
    };
    let exe = std::env::current_exe().map_err(|e| format!("rust-js's own path: {e}"))?;
    let text = std::fs::read_to_string(written).map_err(|e| format!("cannot read the dep-info rustc wrote: {e}"))?;
    // Cargo reads the files of the first `<output>: <files>` line only.
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let first = lines
        .iter_mut()
        .find(|line| !line.starts_with('#') && line.contains(": "))
        .ok_or("rustc's dep-info lists no sources")?;
    first.push(' ');
    first.push_str(&exe.display().to_string().replace(' ', "\\ "));
    records.push((path.to_path_buf(), (lines.join("\n") + "\n").into_bytes()));
    Ok(records)
}

/// Where the manifest of the crate whose metadata is `metadata` is recorded.
fn marker(metadata: &Path) -> PathBuf {
    metadata.with_extension("rust-js")
}

/// Each value of `flag`, given as `--flag value` or `--flag=value`.
fn values<'a>(flags: &'a [String], flag: &'a str) -> impl Iterator<Item = String> + 'a {
    flags.iter().enumerate().filter_map(move |(i, given)| {
        if given == flag {
            flags.get(i + 1).cloned()
        } else {
            given.strip_prefix(flag)?.strip_prefix('=').map(str::to_string)
        }
    })
}

/// Cargo's flags, with each `@file` it wrote for a long command read in.
fn expand(given: &[String]) -> Result<Vec<String>, String> {
    let mut flags = Vec::new();
    for flag in given {
        match flag.strip_prefix('@') {
            Some(file) => {
                let text = std::fs::read_to_string(file).map_err(|e| format!("{file}: {e}"))?;
                flags.extend(text.lines().map(str::to_string));
            }
            None => flags.push(flag.clone()),
        }
    }
    Ok(flags)
}

/// rustc, run as Cargo asked, with what it prints passed through.
fn rustc_itself(rustc: &str, given: &[String]) -> Invocation {
    match std::process::Command::new(rustc).args(given).status() {
        Ok(status) => Invocation::Rustc(match status.code() {
            Some(0) => ExitCode::SUCCESS,
            Some(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
            None => ExitCode::FAILURE,
        }),
        Err(error) => {
            eprintln!("rust-js: cannot run {rustc}: {error}");
            Invocation::Rustc(ExitCode::FAILURE)
        }
    }
}
