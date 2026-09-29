//! The react crate's `cfg`s for one React release (ADR 0043), as `cfg.js`
//! gives `react/build.sh` them: `react="18.1"` and so on for every minor
//! release up to `RUST_JS_REACT`, so an item gated `#[cfg(react = "19.2")]`
//! exists only from React 19.2. The releases are `versions.json`'s.

use std::path::Path;

fn main() {
    let versions = Path::new(env!("CARGO_MANIFEST_DIR")).join("versions.json");
    println!("cargo::rerun-if-changed={}", versions.display());
    println!("cargo::rerun-if-env-changed=RUST_JS_REACT");
    let text = std::fs::read_to_string(&versions).expect("react/versions.json");
    let releases = releases(&text);
    let latest = *releases.last().expect("react/versions.json lists no release");
    let wanted = match std::env::var("RUST_JS_REACT") {
        Ok(version) => minor(&version).unwrap_or_else(|| panic!("RUST_JS_REACT: not a React version: {version}")),
        Err(_) => minor(latest).expect("a release is a version"),
    };
    let first = minor(releases[0]).expect("a release is a version");
    assert!(
        first <= wanted,
        "React {}.{} is older than {}, the first release the react crate supports",
        wanted.0,
        wanted.1,
        releases[0]
    );
    if wanted > minor(latest).expect("a release is a version") {
        println!(
            "cargo::warning=React {}.{} is newer than {latest}, the latest release the react crate knows: it has {latest}'s API",
            wanted.0, wanted.1
        );
    }
    let values: Vec<String> = releases.iter().map(|release| format!("\"{release}\"")).collect();
    println!("cargo::rustc-check-cfg=cfg(react,values({}))", values.join(","));
    for release in releases
        .iter()
        .filter(|release| minor(release).is_some_and(|r| r <= wanted))
    {
        println!("cargo::rustc-cfg=react=\"{release}\"");
    }
}

/// `versions.json`'s releases, `"18.0"` to the latest, as react/generate.ts
/// writes them: the keys of its `"releases"` object, one to a line.
fn releases(text: &str) -> Vec<&str> {
    let start = text.find("\"releases\": {").expect("versions.json has releases");
    let end = start + text[start..].find('}').expect("releases end");
    text[start..end]
        .lines()
        .skip(1)
        .filter_map(|line| line.trim().strip_prefix('"')?.split_once('"').map(|(key, _)| key))
        .collect()
}

/// A version's major and minor release: `18.2.0` is `(18, 2)`.
fn minor(version: &str) -> Option<(u32, u32)> {
    let mut parts = version.split('.');
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}
