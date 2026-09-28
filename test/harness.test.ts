// The harness's own failures, each made on purpose (ADRs 0088, 0089, 0092):
// a compiler that crashes after the rejection a case expects, one that never
// ends, JS that's wrong under one runtime only, a reduction cut short, a
// merge of shards that aren't one run, and a test native Rust never ends.
// Each must fail the run, or say it's incomplete, with what it saw.

import { beforeAll, expect, test } from "bun:test";
import { chmodSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { ratchet, runTest, type Shard } from "../scripts/rustc-suite";
import { buildCompiler, compiler, fixture, root, target } from "./support";

beforeAll(buildCompiler);

/** A compiler at `name` that runs `script`, a shell script. */
function fake(name: string, script: string): string {
  const path = join(fixture(`harness-${name}`), "rust-js");
  writeFileSync(path, `#!/bin/sh\n${script}\n`);
  chmodSync(path, 0o755);
  return path;
}

/** `bun test` of `files`, as another process, with `env`. */
function bunTest(args: string[], env: Record<string, string>) {
  const p = Bun.spawnSync([process.execPath, "test", ...args], { cwd: root, env: { ...process.env, ...env }, stdout: "pipe", stderr: "pipe" });
  return { code: p.exitCode, output: p.stdout.toString() + p.stderr.toString() };
}

test("a compiler that crashes after the rejection a case expects fails it", () => {
  const crashing = fake(
    "crash",
    `echo "error: rust-js does not support 128-bit integers yet" >&2\necho "thread 'rustc' panicked at src/lower.rs:1:1:" >&2\nexit 101`,
  );
  const run = bunTest(["test/corpus.test.ts", "-t", "wider_integers"], { RUST_JS_COMPILER: crashing });
  expect(run.code).not.toBe(0);
  expect(run.output).toContain("rust-js crashed: thread 'rustc' panicked at src/lower.rs:1:1:");
  expect(run.output).toContain("`compile-fail` expects a rejection");
}, 120_000);

test("a compiler that never ends is stopped, and fails the case", () => {
  const hanging = fake("hang", "sleep 60");
  const run = bunTest(["test/corpus.test.ts", "-t", "index_out_of_bounds"], { RUST_JS_COMPILER: hanging, RUST_JS_COMPILE_TIMEOUT: "1000" });
  expect(run.code).not.toBe(0);
  expect(run.output).toContain("rust-js crashed: didn't finish in 1s");
}, 120_000);

// The real compiler's JS, with a line only the JS as compiled runs, not
// the bundle Vite ships of it.
const onlyUnbundled = () =>
  fake("unbundled-only", `"${compiler}" "$@" || exit $?\necho 'if (import.meta.url.endsWith("/case.js")) console.log("only unbundled");' >> "$3"`);

test("JS that's wrong in one way it's run only fails the case", () => {
  const run = bunTest(["test/corpus.test.ts", "-t", "index_out_of_bounds"], { RUST_JS_COMPILER: onlyUnbundled() });
  expect(run.code).not.toBe(0);
  expect(run.output).toMatch(/node stdout:\n\+ only unbundled/);
  expect(run.output).not.toContain("node, minified stdout:");
}, 120_000);

test("a failing generated program is kept before it's reduced, and a reduction cut short says so", () => {
  const out = join(target, "fuzz");
  const run = bunTest(["test/fuzz.test.ts"], { RUST_JS_COMPILER: onlyUnbundled(), FUZZ_START: "3", FUZZ_SEEDS: "1", FUZZ_REDUCE_BUDGET: "1" });
  expect(run.code).not.toBe(0);
  expect(run.output).toContain("differs, reduced for 0.001s, not to the end,");
  const evidence = JSON.parse(readFileSync(join(out, "seed-3.json"), "utf8"));
  expect(evidence).toMatchObject({ seed: 3, kind: "differs", compiler: expect.stringContaining("rust-js") });
  expect(evidence.signature).toBe(JSON.stringify([["node", false, true, true]]));
  expect(evidence.detail).toContain("only unbundled");
  expect(existsSync(join(out, "seed-3.original.rs"))).toBe(true);
  expect(existsSync(join(out, "seed-3.rs"))).toBe(true);
}, 300_000);

test("shards that aren't one whole run aren't checked", () => {
  const dir = fixture("harness-shards");
  const source = Bun.spawnSync(["git", "rev-parse", "HEAD"], { cwd: root }).stdout.toString().trim();
  // Every test the checked-in inventory has, at its rustc commit, as the
  // merge requires.
  const lines = readFileSync(join(root, "test", "rustc-inventory.txt"), "utf8").split("\n");
  const toolchain = lines.find((line) => line.startsWith("# commit: "))!.slice("# commit: ".length);
  const inventory = lines.filter((line) => line && !line.startsWith("#"));
  const shard = (i: number): Shard => {
    const expected = inventory.filter((_, k) => k % 2 === i - 1);
    return { shard: i, of: 2, compiler: "c", toolchain, source, inventory, expected, results: expected.map((test) => ({ test, status: "pass" })) };
  };
  const merge = (...shards: Shard[]) => {
    const files = shards.map((s, k) => {
      const file = join(dir, `results-${k}.json`);
      writeFileSync(file, JSON.stringify(s));
      return file;
    });
    const p = Bun.spawnSync([process.execPath, "scripts/rustc-suite.ts", "--merge", ...files], { cwd: root, stdout: "pipe", stderr: "pipe" });
    return { code: p.exitCode, output: p.stdout.toString() };
  };
  const missing = merge(shard(1));
  expect(missing.code).toBe(1);
  expect(missing.output).toContain("INCOMPLETE\tshards missing, of 2: 2");
  const twice = merge(shard(1), shard(2), shard(2));
  expect(twice.code).toBe(1);
  expect(twice.output).toContain("INCOMPLETE\tshards there more than once, of 2: 2");
  // Whole, it's checked: every listed failure passes, which isn't as listed.
  const whole = merge(shard(1), shard(2));
  expect(whole.output).not.toContain("INCOMPLETE");
  expect(whole.output).toContain("PASSES\t");
  expect(whole.code).toBe(1);
}, 120_000);

test("a test native Rust never ends has no answer, and isn't as listed", async () => {
  const ui = fixture("harness-ui");
  const file = join(ui, "forever.rs");
  writeFileSync(file, "//@ run-pass\nfn main() {\n    loop {}\n}\n");
  mkdirSync(join(target, "rustc-suite"), { recursive: true });
  const result = await runTest(ui, file);
  expect(result).toEqual({ test: "forever.rs", status: "native", reason: "doesn't pass natively with overflow checks off: didn't finish in 10s" });
  if (result.status !== "native") throw new Error(`Expected native failure, got ${result.status}`);
  expect(ratchet([result], new Map(), new Map()).unanswered).toEqual([result]);
}, 120_000);

// A batch of generated programs says which seeds: one that isn't a whole
// number fails before any runs, not as a run of none. Found in review.
test("fuzz settings that aren't whole numbers fail the run", () => {
  // A seed is a 32-bit number to the generator, so a batch past 2^32 would
  // repeat seeds, or, past 2^53, not count up at all. Found in review.
  for (const [name, value] of [
    ["FUZZ_START", "invalid"],
    ["FUZZ_SEEDS", "0"],
    ["FUZZ_SEEDS", "1.5"],
    ["FUZZ_REDUCE_BUDGET", "-1"],
    ["FUZZ_START", "9007199254740992"],
    ["FUZZ_START", "4294967296"],
  ]) {
    const run = bunTest(["test/fuzz.test.ts"], { FUZZ_START: "1", FUZZ_SEEDS: "1", [name]: value });
    expect([name, run.code]).toEqual([name, 1]);
    expect(run.output).toContain(`${name}=${value}: `);
  }
  const past = bunTest(["test/fuzz.test.ts"], { FUZZ_START: "4294967290", FUZZ_SEEDS: "10" });
  expect(past.code).toBe(1);
  expect(past.output).toContain("FUZZ_START=4294967290 FUZZ_SEEDS=10: the seeds end past 4294967296");
}, 120_000);

// A test listed as printing what changes is run natively until it does, up
// to 30 times: a coin flip, as a two-entry HashMap's order is, is the same
// three runs in four, and would look like it answers. Found by a run in
// which rustc's issue-3559.rs did.
test("a test that prints what changes, listed so, is run until it does", async () => {
  const ui = fixture("harness-changing");
  const file = join(ui, "coin.rs");
  writeFileSync(
    file,
    "//@ run-pass\nuse std::collections::hash_map::RandomState;\nuse std::hash::{BuildHasher, Hasher};\nfn main() {\n    let flip = RandomState::new().build_hasher().finish() % 2;\n    println!(\"{flip}\");\n}\n",
  );
  mkdirSync(join(target, "rustc-suite"), { recursive: true });
  const listed = new Set(["coin.rs"]);
  for (let i = 0; i < 3; i++) {
    expect(await runTest(ui, file, listed)).toEqual({ test: "coin.rs", status: "native", reason: "prints what changes from run to run" });
  }
}, 120_000);

/** The real compiler, with JS that prints the bytes FF 0A in its place,
 * which read as UTF-8 are "\u{FFFD}\n", as it prints to stderr. */
function notUtf8(): string {
  const js = join(fixture("harness-not-utf8-js"), "case.js");
  writeFileSync(js, `export function entry() {\n  process.stdout.write(new Uint8Array([0xff, 10]));\n  process.stderr.write("\\uFFFD\\n");\n}\n`);
  return fake("not-utf8", `"${compiler}" "$@" || exit $?\ncp "${js}" "$3"`);
}

// What a case printed is compared as bytes, not as the text they read as.
// Found in review: `agree` compared bytes, but the corpus's own checks,
// rustc's tests', and whether native Rust prints what changes, text.
test("JS that prints other bytes than native Rust, reading the same, fails the case", () => {
  const run = bunTest(["test/corpus.test.ts", "-t", "replacement_character"], { RUST_JS_COMPILER: notUtf8(), RUST_JS_SNAPSHOTS: "ignore" });
  expect(run.code).not.toBe(0);
  expect(run.output).toContain("node stdout:");
  expect(run.output).not.toContain("stderr:");
}, 120_000);

test("a rustc test whose JS prints other bytes, reading the same, fails, and one natively printing either changes", async () => {
  const ui = fixture("harness-bytes");
  const file = join(ui, "replacement.rs");
  writeFileSync(file, `//@ run-pass\nfn main() {\n    println!("\\u{FFFD}");\n    eprintln!("\\u{FFFD}");\n}\n`);
  mkdirSync(join(target, "rustc-suite"), { recursive: true });
  expect(await runTest(ui, file, new Set(), notUtf8())).toMatchObject({ test: "replacement.rs", status: "fail", reason: "node: different stdout" });
  const either = join(ui, "either.rs");
  writeFileSync(
    either,
    "//@ run-pass\nuse std::collections::hash_map::RandomState;\nuse std::hash::{BuildHasher, Hasher};\nuse std::io::Write;\nfn main() {\n    if RandomState::new().build_hasher().finish() % 2 == 0 {\n        std::io::stdout().write_all(&[0xff, b'\\n']).unwrap();\n    } else {\n        println!(\"\\u{FFFD}\");\n    }\n}\n",
  );
  expect(await runTest(ui, either, new Set(["either.rs"]))).toEqual({ test: "either.rs", status: "native", reason: "prints what changes from run to run" });
}, 120_000);
