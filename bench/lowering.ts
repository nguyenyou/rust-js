// End-to-end compiler scaling: sparse cyclic module graphs. Run with Bun.
// An optional compiler path allows comparisons against a saved baseline.
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dir, "..");
const compiler = resolve(process.argv[2] ?? join(root, "target/debug/rust-js"));
mkdirSync(join(root, "target"), { recursive: true });
const dir = mkdtempSync(join(root, "target/lowering-bench-"));
for (const count of [10, 100, 500]) {
  const source = join(dir, `crate_${count}.rs`);
  const output = join(dir, String(count), "lib.js");
  writeFileSync(source, Array.from({ length: count }, (_, i) => `
    pub mod m${i} {
      pub fn value(n: u32) -> u32 { if n == 0 { ${i} } else { super::m${(i + 1) % count}::value(n - 1) } }
    }`).join("\n"));
  const samples: number[] = [];
  for (let i = 0; i < 4; i++) {
    const start = performance.now();
    const p = Bun.spawnSync([compiler, source, "-o", output], { cwd: root, stderr: "pipe" });
    if (p.exitCode !== 0) throw new Error(p.stderr.toString());
    if (i > 0) samples.push(performance.now() - start); // Warm filesystem; includes rustc, lowering, formatting, publication.
  }
  samples.sort((a, b) => a - b);
  console.log(JSON.stringify({ modules: count, medianMs: Math.round(samples[1]), samplesMs: samples.map(Math.round) }));
}
