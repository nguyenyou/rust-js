# Build hosts

`build.js` prepares and invokes the native compiler. Vite delegates to it; other
hosts can use it without importing the Vite plugin.

```js
import { createNativeBuilder } from "rust-js-build/build";

const builder = createNativeBuilder({
  root: "/absolute/app",
  rustJs: "/absolute/toolchain/rust-js",
  resources: "/absolute/rust-js-resources",
  cacheDir: "/absolute/app/.cache/rust-js",
  bindings: [],
  externs: { web: "/absolute/metadata/libweb.rmeta" },
  rustcFlags: [],
});
await builder.compile({
  crate: "/absolute/app/src/lib.rs",
  output: "/absolute/app/generated/lib.js",
  manifest: "/absolute/app/.cache/rust-js/manifest.json",
});
```

`rust-js-build` exposes three package entry points: `/build` for native compiler
preparation, `/manifest` for build-result validation, and `/publish` for WASI
artifact publication. Vite and the playground declare this package dependency;
neither imports tooling through a path outside its own package.

Both host packages remain private while distribution is being developed. Local
tarballs can be made with `bun pm pack` from `tooling/` and `vite-plugin/`.
The package test unpacks these tarballs into an independent application's
`node_modules` and compiles through the plugin. No registry release is implied.
When using unpacked packages, provide `rustJs` explicitly and provide `resources`
when preparing built-in bindings. Defaults locate the development checkout;
compiler binaries and binding resources are not included in these host packages.

Create a separate resource tarball from the repository root:

```sh
bun run pack:resources /absolute/artifacts/rust-js-resources.tgz
```

This stages `rust-js-resources` with the compiler's version, root toolchain pin,
React/web binding sources and build scripts, and the locked Serde manifest and
source. The resource package and metadata cache use the same input inventory.
Unpack it and set `resources` to the directory containing its `package.json` and
`rust-toolchain.toml`. Build outputs go to the configured cache, outside the
resource directory. The resource tarball contains source inputs, not prebuilt
metadata, compiler binaries, or a Rust sysroot. Building still requires Bun,
Bash, and the pinned Rust toolchain; Cargo also needs its locked dependencies
available locally or through its configured registry.

The isolated package test exercises both React/JSX and Serde using these unpacked
resources. Native compiler installation, release checksums, platform qualification,
and published package installation remain separate distribution work.

For packaged resources, the adapter queries `rust-js --version-json` before
preparing metadata. The response contains `version`, `toolchain`, and `abi`,
matching the compiler identity in emitted manifests. The adapter requires the
resource package's version and Rust pin to match, and accepts only ABI 1.
Mismatch errors report both identities and leave existing output untouched.
`rust-js --version` remains the human-readable form. Source-checkout resources
without the `rust-js-resources` package identity retain the development workflow.
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
