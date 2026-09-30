# Build hosts

`build.js` prepares and invokes the native compiler. Vite delegates to it; other
hosts can use it without importing the Vite plugin.

Distributed tooling supports Node.js and Bun using standard Node.js APIs. Bun
is used to develop, test, and create packages in this repository; users do not
need it to run installed packages. Binding preparation and the native launcher
use the host's current JavaScript runtime.

```js
import { createNativeBuilder } from "@rust-js/build/build";

const builder = createNativeBuilder({
  root: "/absolute/app",
  rustJs: "/absolute/toolchain/rust-js",
  resources: "/absolute/rust-js-resources",
  cacheDir: "/absolute/app/.cache/rust-js",
  bindings: [],
  externs: { web: "/absolute/metadata/libwebapi.rmeta" },
  rustcFlags: [],
});
await builder.compile({
  crate: "/absolute/app/src/lib.rs",
  output: "/absolute/app/generated/lib.js",
  manifest: "/absolute/app/.cache/rust-js/manifest.json",
});
```

`@rust-js/build` exposes package entry points: `/build` for native compiler
preparation, `/manifest` for build-result validation, and `/publish` for WASI
artifact publication. Vite and the playground declare this package dependency;
neither imports tooling through a path outside its own package.

Both host packages remain private while distribution is being developed. Local
tarballs can be made with `bun pm pack` from `tooling/` and `vite-plugin/`.
The package test installs these tarballs with Bun into an independent application
and compiles through the plugin. No registry release is implied.
Compiler discovery first uses the supplied `rustJs` path, then the application's
`@rust-js/native` package, then the development checkout. Resource discovery uses the supplied `resources`
path, then `@rust-js/resources` resolved from the application's dependencies,
then the development checkout;
compiler binaries and binding resources are not included in these host packages.

Build and package a native compiler for the current macOS or Linux host:

```sh
cargo build --release --locked
bun run pack:compiler target/release/rust-js /absolute/artifacts/native.tgz
```

The private `@rust-js/native` package contains the binary and a JavaScript launcher,
with OS/architecture restrictions in its package manifest. Install the pinned
Rust toolchain, its minimal profile and the `wasm32-unknown-unknown` target, on
the destination machine first. The launcher
asks that toolchain for its sysroot and sets the dynamic-library search path
before forwarding arguments and exit status to the compiler. It does not install
toolchains or modify global configuration. The adapter discovers this package
automatically. To override it, set `rustJs` to an absolute compiler or launcher
path, including its installed `.bin` link.
The host hashes and watches both the launcher and its compiler binary.

This is a local packaging path, tested on the development host. It still depends
on Node.js or Bun, rustup, and compatible native system libraries. Clean-machine testing,
Linux/macOS version qualification and signing remain open;
the archive is not a standalone portable compiler distribution.

Create a separate resource tarball from the repository root:

```sh
bun run pack:resources /absolute/artifacts/resources.tgz
```

This stages `@rust-js/resources` with the compiler's version, root toolchain pin,
React/web binding sources and build scripts, and the locked Serde manifest and
source. The resource package and metadata cache use the same input inventory.
Install it as `@rust-js/resources`, or unpack it and set `resources` to the
directory containing its `package.json` and `rust-toolchain.toml`.
Build outputs go to the configured cache, outside the
resource directory. The resource tarball contains source inputs, not prebuilt
metadata, compiler binaries, or a Rust sysroot. Building still requires Node.js or Bun,
Bash, and the pinned Rust toolchain; Cargo also needs its locked dependencies
available locally or through its configured registry.

To assemble all four packages together, use a new output directory:

```sh
bun run pack:distribution target/release/rust-js /absolute/artifacts
cd /absolute/artifacts
shasum -a 256 -c SHA256SUMS
```

On Linux, `sha256sum -c SHA256SUMS` also works. The command checks package versions
against the compiler, stages every tarball, writes `distribution.json` with the
compiler identity, host platform/architecture and archive hashes, then renames
the completed directory into place. Existing destinations are rejected and a
failed build removes its staging directory. `SHA256SUMS` covers all four archives
and the distribution manifest. These hashes detect corruption; they do not
authenticate the publisher. The command does not publish or install anything.

For a local installation, add the four tarballs to the application's
`package.json` using paths relative to that file, then install with the application's
package manager. The local tarball setup below is tested with `bun install`;
running the installed packages does not depend on that choice:

```json
{
  "devDependencies": {
    "vite": "8.3.0",
    "@rust-js/vite-plugin": "./artifacts/vite-plugin.tgz",
    "@rust-js/build": "./artifacts/build.tgz",
    "@rust-js/native": "./artifacts/native.tgz",
    "@rust-js/resources": "./artifacts/resources.tgz"
  },
  "overrides": {
    "@rust-js/build": "./artifacts/build.tgz"
  }
}
```

The override routes the plugin's versioned dependency to the local tarball while
the package is unpublished. Select `bindings: ["react", "serde"]`; neither a
compiler path nor a resource path is needed for this installation:

```js
rustJs({ crates: ["src/App.rs"], bindings: ["react", "serde"] })
```

The isolated package test installs offline with lifecycle scripts disabled,
repeats installation with a frozen lockfile, and exercises React/JSX and Serde
using automatic resource discovery. It omits Vite's peer for its direct hook
test; the separate Vite suite exercises the real bundler and Fast Refresh.
The installed launcher, host, and binding preparation are exercised under both
Node.js and Bun, with a failing stub for the other runtime on `PATH` so a hidden
dependency fails the test. The CLI uses a Node.js shebang; Bun-only users can run
`bun node_modules/.bin/rust-js`. The build adapter invokes it with its own runtime.
Direct use of `react/build.sh` defaults to Node.js; set `RUST_JS_JS_RUNTIME` to a
Bun executable to run it with Bun. Hosts supply this automatically.
Clean-machine compiler installation, release authentication, platform qualification,
and published package installation remain separate distribution work.

For packaged resources, the adapter queries `rust-js --version-json` before
preparing metadata. The response contains `version`, `toolchain`, and `abi`,
matching the compiler identity in emitted manifests. The adapter requires the
resource package's version and Rust pin to match, and accepts only ABI 1.
Mismatch errors report both identities and leave existing output untouched.
`rust-js --version` remains the human-readable form. Source-checkout resources
without the `@rust-js/resources` package identity retain the development workflow.
Matching version fields are a compatibility check, not proof of artifact
provenance; release checksums and qualification are still needed.

Use absolute compiler, resource, cache and extern paths. Crate and output paths
may also be relative to `root`. `watchFiles` lists toolchain and binding inputs;
add the validated manifest's `sources` for the application's watch set.

`bindings` defaults to `["react"]`. This builds the web and React metadata for
the application's installed React version. With no installed React, it uses the
binding resource's default. The compiler bytes, binding source inputs, resource
root, React version and compiler options identify the metadata cache directory.
A build failure leaves no completion marker. Deleting the cache is safe.

Use `bindings: ["react", "serde"]` for a React application that also uses
`serde` derives and `serde_json`, or `["serde"]` for a non-React application.
The adapter builds the locked Serde dependency set with the resource bundle's
pinned toolchain and obtains artifact paths from Cargo's JSON output. Cargo
checks freshness on every preparation; changes to the manifest, lockfile,
binding source, compiler or options select a new cache directory. Paths with
spaces are supported. Vite accepts the same `bindings` option:

```js
rustJs({ crates: ["src/App.rs"], bindings: ["react", "serde"] })
```

React and Serde are the built-in preparation recipes. Supply other matching metadata
through `externs`; this does not make arbitrary dependency implementations
available to the JavaScript backend. General Cargo dependency resolution and
cross-crate JavaScript linking remain unsupported. The resource bundle currently
uses the repository's binding build scripts and source layout; a standalone
installable distribution is not yet provided.

## Share model source with native Rust

For now, compile shared source as a module in each target. For example, keep
`shared/model.rs` beside `client/lib.rs` and `server/main.rs`, and include it
from both entry points:

```rust
#[path = "../shared/model.rs"]
mod model;
```

The shared module can contain supported Serde models and portable validation
functions. Keep native I/O in the server entry point and browser APIs in the
client. Configure the client builder with `bindings: ["serde"]` (plus `"react"`
for React). The native server uses ordinary Serde dependencies; use matching
versions and derive features when comparing JSON contracts.

The compiler manifest lists the loaded shared file in `sources`. Watch that
list and rebuild the client when shared source changes; rebuild the native
server separately. Arbitrary Cargo dependencies, build scripts, features and
procedural macros are not automatically prepared by this adapter. Its Serde
recipe supplies the bundled dependency versions and derive support.

[`test/shared-code.test.ts`](../test/shared-code.test.ts) creates an app outside
the repository, compiles the same models and validation for both targets, and
passes client-produced JSON through a native executable and back to the client.
It checks malformed requests and rebuilds both targets after changing a shared
validation rule. The test uses a subprocess for transport; HTTP, a browser UI,
and a deployed full-stack pilot remain separate integration work.

`manifest.js` validates the compiler's version-1 build result and remaps structured
path fields. `publish.js` commits a completed WASI result to the host filesystem.
Callers supply only a successful compiler result and serialize builds targeting
the same outputs. Publication preserves unchanged files, checks stale-file
ownership, and rolls back ordinary I/O errors. It does not guarantee crash-atomic
multi-file replacement.

## Build a Cargo workspace

Cargo builds a workspace's libraries with rust-js as its workspace wrapper
([ADR 0101](../docs/decisions/0101-cargo-workspace-wrapper.md)), each to JS of
its own ([ADR 0100](../docs/decisions/0100-separate-crates.md)):

```js
import { checkCargo } from "@rust-js/build/cargo";

const { js, crates } = await checkCargo({
  manifestPath: "Cargo.toml", toolchain: "<pinned nightly>", compiler: "/path/to/rust-js",
  packageName: "frontend", features: [],
});
```

It runs `RUSTC_WORKSPACE_WRAPPER=<compiler> cargo check --target
wasm32-unknown-unknown`. Each library of the workspace that `frontend` uses,
and `frontend` itself, is JS in Cargo's target directory, beside the metadata
of that build of it, importing the others; `js` is `frontend`'s, and `crates`
each crate's JS and manifest. A feature set built before is the JS it was.
Each file a crate's manifest lists is checked, of a build Cargo has as done
too: one gone or edited is refused, and `cargo clean -p <package> --target
wasm32-unknown-unknown` builds it again.
Registry crates, build scripts and procedural macros are built by rustc, as
Cargo asks; of registry crates, only serde's are known to rust-js at run time.
`cargo build` is refused.

A crate using React depends on the bindings in rust-js, and `checkCargo`'s
`react` is the release it's checked for (ADR 0043), the latest otherwise:

```toml
[dependencies]
react = { package = "rust-js-react", path = "/path/to/rust-js/react" }
```

In Vite, the plugin's `cargo` option builds the workspace with `checkCargo`, for
the React the app has installed, and the app imports the package's JS as
`rust-js:<package>`:

```js
// vite.config.js, in the workspace's web/
plugins: [rustJs({ cargo: { package: "frontend", manifestPath: "../Cargo.toml" } }), react()]
```

```js
import { App } from "rust-js:frontend";
```

An edit to any crate of the workspace is one `cargo check`, and the JS it
changes is a Fast Refresh. Each module's JS is written beside its Rust, to be
committed (ADR 0041): `frontend/src/api.rs` is `frontend/src/api.js`. Without
rust-js, Vite builds from those. `cargo: { inSource: false }` leaves the JS in
Cargo's target directory only. Cargo's target directory must be one Vite serves
from: its workspace root, or another of `server.fs.allow`.

## Experimental Cargo planning

`@rust-js/build/cargo` exports `planCargoLibraries({ manifestPath, toolchain, target,
packageName?, features?, noDefaultFeatures? })`. Supply an exact nightly pin and
an explicit target triple. `packageName` selects a member of a virtual workspace.

The planner invokes Cargo metadata with `--frozen`: create and commit the lockfile
first. It resolves offline without changing that lockfile, and returns local
library packages in dependency order, preserving dependency aliases, source paths,
editions and Cargo-resolved features. Development dependencies are excluded.
Registry/git packages, build scripts, procedural macros and packages without an
ordinary library target are rejected when reachable from the selected library.

This is dependency discovery, not a Cargo compilation entry point. It does not
produce rustc invocations, compile dependencies to JS, establish a cross-crate ABI
or cache compilation artifacts. Metadata feature sets are Cargo resolution data,
not a substitute for Cargo compiler-unit/build-script information. The native
builder and Vite do not consume this experimental plan yet.

The compiler also has an experimental scalar linkage proof, described in
[ADR 0085](../docs/decisions/0085-scalar-library-linkage.md). A producer uses
`rust-js shared.rs -o shared.js --library --manifest shared.json`; a consumer
passes `--dependency shared.json` plus real rustc `--extern` metadata after `--`.
This supports only the documented scalar free-function ABI. The caller must build
matching metadata and JS and rebuild dependencies after source edits. The Cargo
planner, native build adapter and Vite do not orchestrate these steps yet.
