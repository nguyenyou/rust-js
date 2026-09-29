# @rust-js/runtime

The helpers the JS rust-js writes imports: bounds checks, Rust's integer
arithmetic and formatting, iterators, JSON for serde, and the rest. Released
with the compiler, at its version, as ReScript's `@rescript/runtime` is. See
[ADR 0103](../docs/decisions/0103-runtime-package.md).

```js
// frontend/src/form.js, as rust-js writes it
import { $debugStr, $parseInt } from "@rust-js/runtime";
```

A module imports the helpers its code names; the package has the ones they
use in turn. Each is a named export, so a bundler keeps only what's used. An
app installs the compiler's own version: the build adapter refuses another.

`index.js` is written by the compiler, from the helpers it has:

```bash
bun run runtime        # ./target/debug/rust-js --runtime-module > runtime/index.js
```
