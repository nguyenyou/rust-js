// End-to-end compiler scaling: sparse cyclic module graphs. Run with Bun.
// An optional compiler path allows comparisons against a saved baseline.
// Each graph is laid out two ways: inline modules in one file, and a file
// for each module, where every module's source map is planned from the
// crate's many sources (found in review: that was modules × sources work).
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dir, "..");
const compiler = resolve(process.argv[2] ?? join(root, "target/debug/rust-js"));
mkdirSync(join(root, "target"), { recursive: true });
const dir = mkdtempSync(join(root, "target/lowering-bench-"));
const body = (i: number, count: number) =>
  `pub fn value(n: u32) -> u32 { if n == 0 { ${i} } else { super::m${(i + 1) % count}::value(n - 1) } }`;
for (const layout of ["inline", "files"] as const) {
  for (const count of [10, 100, 500]) {
    const crate = join(dir, `${layout}_${count}`);
    mkdirSync(crate, { recursive: true });
    const source = join(crate, "lib.rs");
    if (layout === "inline") {
      writeFileSync(source, Array.from({ length: count }, (_, i) => `pub mod m${i} { ${body(i, count)} }`).join("\n"));
    } else {
      writeFileSync(source, Array.from({ length: count }, (_, i) => `pub mod m${i};`).join("\n"));
      for (let i = 0; i < count; i++) writeFileSync(join(crate, `m${i}.rs`), `${body(i, count)}\n`);
    }
    const output = join(crate, "out", "lib.js");
    const samples: number[] = [];
    for (let i = 0; i < 4; i++) {
      const start = performance.now();
      const p = Bun.spawnSync([compiler, source, "-o", output], { cwd: root, stderr: "pipe" });
      if (p.exitCode !== 0) throw new Error(p.stderr.toString());
      if (i > 0) samples.push(performance.now() - start); // Warm filesystem; includes rustc, lowering, formatting, publication.
    }
    samples.sort((a, b) => a - b);
    console.log(JSON.stringify({ layout, modules: count, medianMs: Math.round(samples[1]), samplesMs: samples.map(Math.round) }));
  }
}
