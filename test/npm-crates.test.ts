// The crates rust-js releases on npm, `@rust-js/builtins`, `@rust-js/webapi`
// and `@rust-js/react`: each the crate crates.io would have, Cargo's own
// packaging of it, in an npm package that names it, and what it shares with
// an app a peer dependency. An app installs them, and Cargo finds them in
// its `node_modules` by a patch, as a binding is found.

import { expect, test } from "bun:test";
import { mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { runSync } from "./child";
import { fixture, root, run } from "./support";

const cargo = (dir: string) => Bun.TOML.parse(readFileSync(join(root, dir, "Cargo.toml"), "utf8")) as { package: { name: string; version: string } };

/** Every file under `dir`, relative to it, sorted. */
function files(dir: string): string[] {
  return readdirSync(dir, { recursive: true, withFileTypes: true })
    .filter((entry) => entry.isFile())
    .map((entry) => join(entry.parentPath, entry.name).slice(dir.length + 1))
    .sort();
}

test("builtins, webapi and react are npm packages of their crates, which an app's Cargo finds in node_modules", () => {
  const out = fixture("npm-crates");
  run([process.execPath, "scripts/package-npm-crates.ts", out], 600_000, { RUSTC_BOOTSTRAP: "" });
  expect(readdirSync(out).sort()).toEqual(["builtins.tgz", "react.tgz", "webapi.tgz"]);

  const unpacked = (name: string) => {
    const dir = join(fixture(`npm-crate-${name}`));
    run(["tar", "-xzf", join(out, `${name}.tgz`), "-C", dir]);
    return join(dir, "package");
  };
  const repository = (directory: string) => ({ type: "git", url: "git+https://github.com/rust-js-lang/rust-js.git", directory });
  const crates = { "@rust-js/builtins": "~0.0.1", "@rust-js/webapi": "~0.0.1" };
  for (const [name, peers, crateFiles] of [
    ["builtins", undefined, ["src/lib.rs"]],
    ["webapi", { "@rust-js/builtins": crates["@rust-js/builtins"] }, ["src/lib.rs"]],
    // React's own, the library it binds, and what its build script reads.
    ["react", { ...crates, react: ">=18.0.0", "react-dom": ">=18.0.0" }, ["build.rs", "src/dom.rs", "src/elements.rs", "src/event.rs", "src/lib.rs", "versions.json"]],
  ] as const) {
    const dir = unpacked(name);
    const pkg = JSON.parse(readFileSync(join(dir, "package.json"), "utf8"));
    const crate = cargo(name).package;
    expect(pkg).toMatchObject({
      name: `@rust-js/${name}`,
      version: crate.version,
      license: "MIT",
      repository: repository(name),
      "rust-js": { crate: crate.name },
    });
    expect(pkg.keywords).toContain("rust-js");
    expect(pkg.peerDependencies).toEqual(peers);
    // The crate as Cargo packages it: its Rust, no path to where it was.
    expect(files(dir)).toEqual(["Cargo.toml", "LICENSE", "README.md", "package.json", ...crateFiles].sort());
    expect(readFileSync(join(dir, "Cargo.toml"), "utf8")).not.toContain("path = \"../");
  }

  // An app, as a package manager installs them, and a plain stable Cargo,
  // told where they are by a patch.
  const app = fixture("npm-crates-app");
  mkdirSync(join(app, "src"));
  // webapi's peer dependency on builtins is the app's package, which bun
  // would look for on the registry, as a released one is: here, the
  // override is the one packed.
  const packed = (name: string) => `file:${join(out, `${name}.tgz`)}`;
  writeFileSync(join(app, "package.json"), JSON.stringify({
    private: true,
    dependencies: { "@rust-js/builtins": packed("builtins"), "@rust-js/webapi": packed("webapi"), "@rust-js/react": packed("react") },
    overrides: { "@rust-js/builtins": packed("builtins"), "@rust-js/webapi": packed("webapi") },
  }));
  // React itself, a peer, isn't what Cargo reads: it's left out, and the
  // install needs no registry.
  const installed = runSync([process.execPath, "install", "--ignore-scripts", "--omit", "peer"], app, 120_000, { BUN_INSTALL_CACHE_DIR: join(app, ".cache") });
  expect(installed.stderr).not.toContain("error");
  expect(installed.code).toBe(0);
  const version = (name: string) => `~${cargo(name).package.version}`;
  writeFileSync(join(app, "Cargo.toml"), `[package]
name = "app"
version = "0.0.0"
edition = "2024"

[dependencies]
js = { package = "rust-js-builtins", version = "${version("builtins")}" }
webapi = { package = "rust-js-webapi", version = "${version("webapi")}" }
react = { package = "rust-js-react", version = "${version("react")}" }

[workspace]
`);
  writeFileSync(
    join(app, "src", "lib.rs"),
    "#![allow(non_snake_case)]\nuse react::{Element, jsx};\n\npub fn body(document: &webapi::Document) -> Option<&'static webapi::HtmlElement> {\n    webapi::document::body(document)\n}\n\npub fn App() -> Element {\n    jsx! { <p>{\"from npm\"}</p> }\n}\n",
  );
  mkdirSync(join(app, ".cargo"));
  writeFileSync(join(app, ".cargo", "config.toml"), `[patch.crates-io]
rust-js-builtins = { path = "node_modules/@rust-js/builtins" }
rust-js-webapi = { path = "node_modules/@rust-js/webapi" }
rust-js-react = { path = "node_modules/@rust-js/react" }
`);
  const check = runSync(["cargo", "check", "--offline", "--quiet", "--manifest-path", join(app, "Cargo.toml")], app, 300_000, { RUSTC_BOOTSTRAP: undefined });
  expect(check.stderr).toBe("");
  expect(check.code).toBe(0);
}, 900_000);
