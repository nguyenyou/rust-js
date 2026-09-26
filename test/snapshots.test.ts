// Snapshots of the generated JS (see test/snapshots/): every example and
// fixture, compiled and compared file by file. A compiler change that changes
// its output fails here with the diff; `bun run bless` writes the new output
// as the snapshot, so `git diff` shows what the change did to the JS.

import { beforeAll, test } from "bun:test";
import { rmSync } from "node:fs";
import { basename, join } from "node:path";
import { buildCompiler, buildReact, buildWeb, compiler, expectSnapshot, root, run, target } from "./support";

type Crate = "web" | "react";

// A name (its folder in test/snapshots/), the crate's root, and the crates it uses.
const cases: [string, string, Crate[]][] = [
  ["fib", "examples/fib.rs", []],
  ["structs", "examples/structs.rs", []],
  ["closures", "examples/closures.rs", []],
  ["collections", "examples/collections.rs", []],
  ["options", "examples/options.rs", []],
  ["methods", "examples/methods.rs", []],
  ["traits", "examples/traits.rs", []],
  ["consts", "examples/consts.rs", []],
  ["enums", "examples/enums.rs", []],
  ["strings", "examples/strings.rs", []],
  ["results", "examples/results.rs", []],
  ["iterators", "examples/iterators.rs", []],
  ["thread_locals", "examples/thread_locals.rs", []],
  ["modules", "examples/modules/lib.rs", []],
  ["counter", "examples/counter.rs", ["web"]],
  ["todo", "examples/todo.rs", ["web"]],
  ["countdown", "examples/countdown.rs", ["web"]],
  ["fetch", "examples/fetch.rs", ["web"]],
  ["web_forms", "test/web_forms.rs", ["web"]],
  ["throws", "test/throws.rs", ["web"]],
  ["async", "test/async.rs", ["web"]],
  ["imports", "test/imports/lib.rs", []],
  ["components", "test/components.rs", ["react"]],
  ["apis", "test/apis.rs", ["react"]],
  // The playground's own Rust (ADR 0044). test/playground.test.ts checks that
  // rust-js.wasm, which compiles it for the site, writes the same.
  ["playground", "wasm/web/rust/lib.rs", ["web", "react"]],
];

beforeAll(() => {
  buildCompiler();
  buildWeb();
  buildReact();
}, 600_000);

for (const [name, input, crates] of cases) {
  test(`${name}: the generated JS is its snapshot`, () => {
    const out = join(target, "snapshots", name);
    rmSync(out, { recursive: true, force: true });
    const flags: string[] = [];
    if (crates.includes("web")) flags.push("--extern", `web=${join(target, "libweb.rmeta")}`);
    if (crates.includes("react")) flags.push("--extern", `react=${join(target, "libreact.rmeta")}`, "-L", target);
    // From the repository's root, so each file's header names its source the
    // same way on every machine.
    run([compiler, input, "-o", join(out, `${basename(input, ".rs")}.js`), ...(flags.length > 0 ? ["--", ...flags] : [])]);
    expectSnapshot(out, join(root, "test/snapshots", name));
  });
}
