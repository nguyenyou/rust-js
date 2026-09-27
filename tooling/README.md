# Build hosts

`build.js` prepares and invokes the native compiler. Vite delegates to it; other
hosts can use it without importing the Vite plugin.

```js
import { createNativeBuilder } from "./tooling/build.js";

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

Use absolute compiler, resource, cache and extern paths. Crate and output paths
may also be relative to `root`. `watchFiles` lists toolchain and binding inputs;
add the validated manifest's `sources` for the application's watch set.

`bindings` defaults to `["react"]`. This builds the web and React metadata for
the application's installed React version. With no installed React, it uses the
binding resource's default. The compiler bytes, binding source inputs, resource
root, React version and compiler options identify the metadata cache directory.
A build failure leaves no completion marker. Deleting the cache is safe.

Only React is a built-in preparation recipe. Supply other matching metadata
through `externs`; this does not make arbitrary dependency implementations
available to the JavaScript backend. General Cargo dependency resolution and
cross-crate JavaScript linking remain unsupported. The resource bundle currently
uses the repository's binding build scripts and source layout; a standalone
installable distribution is not yet provided.

`manifest.js` validates the compiler's version-1 build result and remaps structured
path fields. `publish.js` commits a completed WASI result to the host filesystem.
Callers supply only a successful compiler result and serialize builds targeting
the same outputs. Publication preserves unchanged files, checks stale-file
ownership, and rolls back ordinary I/O errors. It does not guarantee crash-atomic
multi-file replacement.
