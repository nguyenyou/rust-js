// The tests' own helpers (ADR 0104): a native program kept for the next
// run is the program its sources make now, and the compiler is built once
// for a run, with no one left waiting on no one.

import { expect, test } from "bun:test";
import { mkdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";

import { runNative } from "./programs";
import { fixture, nativeBinary, once, run } from "./support";

// Found in review: a file the case includes was changed, and the kept
// binary, of the file before, answered for it.
test("a kept native program is built again when a file it includes changes", () => {
  const dir = fixture("native-transitive");
  writeFileSync(join(dir, "cases.rs"), 'pub fn value() -> u32 {\n    include!("value.rs")\n}\n');
  const answer = () => {
    const built = nativeBinary('include!("cases.rs");\nfn main() {\n    println!("{}", value());\n}\n', dir, ["--edition=2024"]);
    if ("error" in built) throw new Error(built.error);
    return run([built.binary]).trim();
  };
  writeFileSync(join(dir, "value.rs"), "1\n");
  expect(answer()).toBe("1");
  writeFileSync(join(dir, "value.rs"), "2\n");
  expect(answer()).toBe("2");
});

// Found in review: the case was copied away from the files beside it.
test("a native case reads the files beside it, as the JS one does", () => {
  const dir = fixture("native-beside");
  const source = join(dir, "case.rs");
  writeFileSync(source, 'fn main() {\n    println!("{} {}", include!("value.rs"), include_str!("value.txt").trim());\n}\n');
  writeFileSync(join(dir, "value.rs"), "7\n");
  writeFileSync(join(dir, "value.txt"), "seven\n");
  const native = runNative(source, fixture("native-beside-run"));
  if (typeof native === "string") throw new Error(native);
  expect(native.stdout).toBe("7 seven\n");
});

test("a kept native program is built again when a variable it reads changes", () => {
  const dir = fixture("native-env");
  const source = 'fn main() {\n    println!("{}", env!("RUST_JS_TEST_VALUE"));\n}\n';
  const answer = (value: string) => {
    process.env.RUST_JS_TEST_VALUE = value;
    try {
      const built = nativeBinary(source, dir, ["--edition=2024"]);
      if ("error" in built) throw new Error(built.error);
      return run([built.binary]).trim();
    } finally {
      delete process.env.RUST_JS_TEST_VALUE;
    }
  };
  expect(answer("one")).toBe("one");
  expect(answer("two")).toBe("two");
});

// Found in review: two tests' wrappers were the same text, `include!("cases.rs")`,
// in two directories, and the second was given the first's binary.
test("the same wrapper in two directories is two programs", () => {
  const answer = (value: string) => {
    const dir = fixture("native-same-wrapper");
    writeFileSync(join(dir, "cases.rs"), `pub fn value() -> u32 {\n    ${value}\n}\n`);
    const built = nativeBinary('include!("cases.rs");\nfn main() {\n    println!("{}", value());\n}\n', dir, ["--edition=2024"]);
    if ("error" in built) throw new Error(built.error);
    return run([built.binary]).trim();
  };
  expect(answer("1")).toBe("1");
  expect(answer("2")).toBe("2");
});

// Found in review: a newer build replaced the one another test file had
// been given, and was about to run.
test("a binary given out stays when a newer one of its program is kept", () => {
  const dir = fixture("native-kept");
  const build = (value: string) => {
    writeFileSync(join(dir, "value.rs"), `${value}\n`);
    const built = nativeBinary('fn main() {\n    println!("{}", include!("value.rs"));\n}\n', dir, ["--edition=2024"]);
    if ("error" in built) throw new Error(built.error);
    return built.binary;
  };
  const first = build("1");
  const second = build("2");
  expect(second).not.toBe(first);
  expect(run([first]).trim()).toBe("1");
  expect(run([second]).trim()).toBe("2");
});

// The compiler's build, once for a run: one process does it, those at the
// same time wait for it, and none takes it over.
test("work claimed once runs once, however many ask at the same time", async () => {
  const dir = join(fixture("once"), "claim");
  const counter = join(dirname(dir), "count");
  const script = `import { appendFileSync } from "node:fs";
import { once } from ${JSON.stringify(join(import.meta.dir, "support.ts"))};
once(${JSON.stringify(dir)}, () => { appendFileSync(${JSON.stringify(counter)}, "ran\\n"); Bun.sleepSync(500); });
`;
  const runs = [0, 1, 2].map(() => Bun.spawn(["bun", "-e", script], { stderr: "pipe" }));
  expect(await Promise.all(runs.map((p) => p.exited))).toEqual([0, 0, 0]);
  expect(readFileSync(counter, "utf8")).toBe("ran\n");
});

// Found in review: a lock made, and its maker stopped before it said who it
// was, and every run after waited for it. A claim is a run's own, and one
// whose worker has ended is an error, not a wait, and not taken over.
test("work claimed by a process that has ended is an error, not a wait", () => {
  const dir = join(fixture("once-ended"), "claim");
  mkdirSync(dir);
  writeFileSync(join(dir, "pid"), "999999");
  expect(() => once(dir, () => "ran")).toThrow("ended before it finished");
});

// Found in review: two test files building the same program shared its
// source, and one's rustc read it as the other rewrote it, empty. It's
// replaced whole, a new file moved into place, never rewritten in place.
test("a native program's source is replaced whole, not rewritten in place", () => {
  const dir = fixture("native-whole");
  const source = 'fn main() {\n    println!("whole");\n}\n';
  const built = nativeBinary(source, dir, ["--edition=2024"]);
  if ("error" in built) throw new Error(built.error);
  const before = statSync(join(dir, "native.rs")).ino;
  nativeBinary(source, dir, ["--edition=2024"]);
  expect(statSync(join(dir, "native.rs")).ino).not.toBe(before);
  expect(readFileSync(join(dir, "native.rs"), "utf8")).toBe(source);
});

// Found in review: a file that never built the compiler ran the formatter,
// which built it itself, and Cargo, linking it again, took it away from the
// files running beside it. Each file is given it before it runs: this one
// builds nothing.
test("every test file is given the compiler before its tests run", () => {
  expect(process.env.RUST_JS_COMPILER).toBeTruthy();
});
